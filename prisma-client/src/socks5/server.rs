use std::net::{Ipv4Addr, Ipv6Addr};
use std::sync::Arc;

use anyhow::Result;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tracing::{debug, info, warn};

use prisma_core::dns::DnsMode;
use prisma_core::router::RouteAction;
use prisma_core::types::{ProxyAddress, ProxyDestination};

use crate::proxy::ProxyContext;
use crate::relay;
use crate::tunnel;
use crate::udp_relay;

/// RFC 1928 SOCKS5 server.
pub async fn run_socks5_server(listen_addr: &str, ctx: ProxyContext) -> Result<()> {
    let listener = TcpListener::bind(listen_addr).await?;
    info!(addr = %listen_addr, "SOCKS5 server started");

    loop {
        match listener.accept().await {
            Ok((stream, peer)) => {
                let ctx = ctx.clone();
                debug!(peer = %peer, "SOCKS5 client connected");
                tokio::spawn(async move {
                    if let Err(e) = handle_socks5_client(stream, &ctx).await {
                        warn!(peer = %peer, error = %e, "SOCKS5 session error");
                    }
                });
            }
            Err(e) => {
                warn!(error = %e, "Failed to accept SOCKS5 connection");
            }
        }
    }
}

async fn handle_socks5_client(mut stream: TcpStream, ctx: &ProxyContext) -> Result<()> {
    // === Phase 1: Method negotiation ===
    // Client: [VER:1][NMETHODS:1][METHODS:1-255]
    let mut buf = [0u8; 258];
    let ver = read_u8(&mut stream).await?;
    if ver != 0x05 {
        return Err(anyhow::anyhow!("Unsupported SOCKS version: {}", ver));
    }
    let nmethods = read_u8(&mut stream).await? as usize;
    stream.read_exact(&mut buf[..nmethods]).await?;

    // We only support no-auth (0x00)
    let has_noauth = buf[..nmethods].contains(&0x00);
    if !has_noauth {
        stream.write_all(&[0x05, 0xFF]).await?;
        return Err(anyhow::anyhow!("Client doesn't support no-auth"));
    }

    // Server: [VER:1][METHOD:1] — selected method 0x00 (no auth)
    stream.write_all(&[0x05, 0x00]).await?;

    // === Phase 2: Request ===
    // Client: [VER:1][CMD:1][RSV:1][ATYP:1][DST.ADDR:var][DST.PORT:2]
    let ver = read_u8(&mut stream).await?;
    if ver != 0x05 {
        return Err(anyhow::anyhow!("Unsupported SOCKS version in request"));
    }
    let cmd = read_u8(&mut stream).await?;
    let _rsv = read_u8(&mut stream).await?; // reserved
    let atyp = read_u8(&mut stream).await?;

    let destination = parse_address(&mut stream, atyp).await?;

    match cmd {
        0x01 => {
            // CONNECT
            handle_connect(stream, ctx, destination).await
        }
        0x03 => {
            // UDP ASSOCIATE
            handle_udp_associate(stream, ctx).await
        }
        _ => {
            send_socks5_reply(&mut stream, 0x07).await?;
            Err(anyhow::anyhow!("Unsupported SOCKS5 command: {}", cmd))
        }
    }
}

/// Handle SOCKS5 CONNECT command (TCP proxy).
async fn handle_connect(
    mut stream: TcpStream,
    ctx: &ProxyContext,
    destination: ProxyDestination,
) -> Result<()> {
    // Resolve fake IP back to domain if in Fake DNS mode
    let destination = resolve_fake_ip(ctx, destination).await;

    // Check routing rules
    let domain = match &destination.address {
        ProxyAddress::Domain(d) => Some(d.as_str()),
        _ => None,
    };
    let ip = match &destination.address {
        ProxyAddress::Ipv4(ip) => Some(std::net::IpAddr::V4(*ip)),
        ProxyAddress::Ipv6(ip) => Some(std::net::IpAddr::V6(*ip)),
        _ => None,
    };

    // Smart DNS: domains that should be tunneled are always proxied
    let force_proxy = domain.is_some_and(|d| ctx.dns_resolver.should_tunnel_dns(d));

    match ctx.router.route(domain, ip, destination.port) {
        RouteAction::Block => {
            info!(dest = %destination, "SOCKS5 CONNECT blocked by routing rule");
            send_socks5_reply(&mut stream, 0x02).await?; // connection not allowed
            return Ok(());
        }
        RouteAction::Direct if !force_proxy => {
            info!(dest = %destination, "SOCKS5 CONNECT direct (bypassing proxy)");
            let dest_str = match &destination.address {
                ProxyAddress::Domain(d) => format!("{}:{}", d, destination.port),
                ProxyAddress::Ipv4(ip) => format!("{}:{}", ip, destination.port),
                ProxyAddress::Ipv6(ip) => format!("[{}]:{}", ip, destination.port),
            };
            let outbound = TcpStream::connect(&dest_str).await?;
            send_socks5_reply(&mut stream, 0x00).await?;
            return relay::relay_direct(stream, outbound).await;
        }
        RouteAction::Direct => {
            // force_proxy=true: Smart DNS says this domain is blocked,
            // override Direct routing and proxy it anyway
            debug!(dest = %destination, "Smart DNS overriding Direct route to Proxy");
        }
        RouteAction::Proxy => {
            // Fall through to tunnel
        }
    }

    info!(dest = %destination, "SOCKS5 CONNECT");

    let tunnel_stream = ctx.connect().await?;

    let tunnel_conn = tunnel::establish_tunnel(
        tunnel_stream,
        ctx.client_id,
        ctx.auth_secret,
        ctx.cipher_suite,
        &destination,
    )
    .await?;

    // Send success reply to SOCKS5 client
    send_socks5_reply(&mut stream, 0x00).await?;

    // Relay data
    relay::relay(stream, tunnel_conn).await
}

/// If the destination is a fake IP, resolve it back to the real domain.
async fn resolve_fake_ip(ctx: &ProxyContext, dest: ProxyDestination) -> ProxyDestination {
    if *ctx.dns_resolver.mode() != DnsMode::Fake {
        return dest;
    }

    if let ProxyAddress::Ipv4(ip) = &dest.address {
        if let Some(domain) = ctx.dns_resolver.lookup_fake_ip(*ip).await {
            debug!(ip = %ip, domain = %domain, "Resolved fake IP back to domain");
            return ProxyDestination {
                address: ProxyAddress::Domain(domain),
                port: dest.port,
            };
        }
    }

    dest
}

/// Handle SOCKS5 UDP ASSOCIATE command (RFC 1928 Section 7).
///
/// Flow:
/// 1. Bind a local UDP socket for the SOCKS5 client to send datagrams to
/// 2. Reply with the bound address/port
/// 3. Establish a UDP tunnel to the Prisma server
/// 4. Relay datagrams between local UDP socket and encrypted tunnel
/// 5. When the TCP control connection closes, tear down the UDP relay
async fn handle_udp_associate(mut stream: TcpStream, ctx: &ProxyContext) -> Result<()> {
    info!("SOCKS5 UDP ASSOCIATE");

    // Bind a local UDP socket on an ephemeral port
    let udp_socket = UdpSocket::bind("127.0.0.1:0").await?;
    let local_addr = udp_socket.local_addr()?;
    info!(addr = %local_addr, "UDP relay socket bound");

    // Send SOCKS5 reply with the bound address
    send_socks5_reply_with_addr(&mut stream, 0x00, local_addr).await?;

    // Connect to the Prisma server and establish a UDP tunnel
    let tunnel_stream = ctx.connect().await?;
    let tunnel_conn = tunnel::establish_udp_tunnel(
        tunnel_stream,
        ctx.client_id,
        ctx.auth_secret,
        ctx.cipher_suite,
    )
    .await?;

    let udp_socket = Arc::new(udp_socket);

    // Run the UDP relay. When the TCP control connection closes, stop.
    let fec_config = ctx.udp_fec.clone();
    let relay_handle = tokio::spawn({
        let udp_socket = udp_socket.clone();
        async move { udp_relay::relay_udp(udp_socket, tunnel_conn, fec_config).await }
    });

    // Monitor the TCP control connection — when it closes, the UDP relay should stop.
    // Per RFC 1928: "A UDP association terminates when the TCP connection that the
    // UDP ASSOCIATE request arrived on terminates."
    let mut control_buf = [0u8; 1];
    let _ = stream.read(&mut control_buf).await;

    relay_handle.abort();
    debug!("SOCKS5 UDP ASSOCIATE session ended (TCP control closed)");
    Ok(())
}

/// Parse SOCKS5 address from stream.
async fn parse_address(stream: &mut TcpStream, atyp: u8) -> Result<ProxyDestination> {
    match atyp {
        0x01 => {
            let mut addr = [0u8; 4];
            stream.read_exact(&mut addr).await?;
            let port = read_u16(stream).await?;
            Ok(ProxyDestination {
                address: ProxyAddress::Ipv4(Ipv4Addr::from(addr)),
                port,
            })
        }
        0x03 => {
            let len = read_u8(stream).await? as usize;
            let mut domain_buf = vec![0u8; len];
            stream.read_exact(&mut domain_buf).await?;
            let domain = String::from_utf8(domain_buf)?;
            let port = read_u16(stream).await?;
            Ok(ProxyDestination {
                address: ProxyAddress::Domain(domain),
                port,
            })
        }
        0x04 => {
            let mut addr = [0u8; 16];
            stream.read_exact(&mut addr).await?;
            let port = read_u16(stream).await?;
            Ok(ProxyDestination {
                address: ProxyAddress::Ipv6(Ipv6Addr::from(addr)),
                port,
            })
        }
        _ => Err(anyhow::anyhow!("Unsupported address type: {}", atyp)),
    }
}

async fn send_socks5_reply(stream: &mut TcpStream, rep: u8) -> Result<()> {
    let reply = [
        0x05, rep, 0x00, 0x01, // VER, REP, RSV, ATYP (IPv4)
        0x00, 0x00, 0x00, 0x00, // BND.ADDR (0.0.0.0)
        0x00, 0x00, // BND.PORT (0)
    ];
    stream.write_all(&reply).await?;
    Ok(())
}

/// Send SOCKS5 reply with a specific bound address (for UDP ASSOCIATE).
async fn send_socks5_reply_with_addr(
    stream: &mut TcpStream,
    rep: u8,
    addr: std::net::SocketAddr,
) -> Result<()> {
    match addr {
        std::net::SocketAddr::V4(v4) => {
            let mut reply = vec![0x05, rep, 0x00, 0x01]; // VER, REP, RSV, ATYP=IPv4
            reply.extend_from_slice(&v4.ip().octets());
            reply.extend_from_slice(&v4.port().to_be_bytes());
            stream.write_all(&reply).await?;
        }
        std::net::SocketAddr::V6(v6) => {
            let mut reply = vec![0x05, rep, 0x00, 0x04]; // VER, REP, RSV, ATYP=IPv6
            reply.extend_from_slice(&v6.ip().octets());
            reply.extend_from_slice(&v6.port().to_be_bytes());
            stream.write_all(&reply).await?;
        }
    }
    Ok(())
}

async fn read_u8(stream: &mut TcpStream) -> Result<u8> {
    let mut buf = [0u8; 1];
    stream.read_exact(&mut buf).await?;
    Ok(buf[0])
}

async fn read_u16(stream: &mut TcpStream) -> Result<u16> {
    let mut buf = [0u8; 2];
    stream.read_exact(&mut buf).await?;
    Ok(u16::from_be_bytes(buf))
}
