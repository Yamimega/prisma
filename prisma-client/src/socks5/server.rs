use std::net::{Ipv4Addr, Ipv6Addr};

use anyhow::Result;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tracing::{debug, info, warn};

use prisma_core::types::{ProxyAddress, ProxyDestination};

use crate::connector;
use crate::proxy::ProxyContext;
use crate::relay;
use crate::tunnel;

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

    // === Phase 2: CONNECT request ===
    // Client: [VER:1][CMD:1][RSV:1][ATYP:1][DST.ADDR:var][DST.PORT:2]
    let ver = read_u8(&mut stream).await?;
    if ver != 0x05 {
        return Err(anyhow::anyhow!("Unsupported SOCKS version in request"));
    }
    let cmd = read_u8(&mut stream).await?;
    let _rsv = read_u8(&mut stream).await?; // reserved
    let atyp = read_u8(&mut stream).await?;

    if cmd != 0x01 {
        send_socks5_reply(&mut stream, 0x07).await?;
        return Err(anyhow::anyhow!("Unsupported SOCKS5 command: {}", cmd));
    }

    let destination = match atyp {
        0x01 => {
            let mut addr = [0u8; 4];
            stream.read_exact(&mut addr).await?;
            let port = read_u16(&mut stream).await?;
            ProxyDestination {
                address: ProxyAddress::Ipv4(Ipv4Addr::from(addr)),
                port,
            }
        }
        0x03 => {
            let len = read_u8(&mut stream).await? as usize;
            let mut domain_buf = vec![0u8; len];
            stream.read_exact(&mut domain_buf).await?;
            let domain = String::from_utf8(domain_buf)?;
            let port = read_u16(&mut stream).await?;
            ProxyDestination {
                address: ProxyAddress::Domain(domain),
                port,
            }
        }
        0x04 => {
            let mut addr = [0u8; 16];
            stream.read_exact(&mut addr).await?;
            let port = read_u16(&mut stream).await?;
            ProxyDestination {
                address: ProxyAddress::Ipv6(Ipv6Addr::from(addr)),
                port,
            }
        }
        _ => {
            send_socks5_reply(&mut stream, 0x08).await?;
            return Err(anyhow::anyhow!("Unsupported address type: {}", atyp));
        }
    };

    info!(dest = %destination, "SOCKS5 CONNECT");

    // === Phase 3: Connect to remote Prisma server and establish tunnel ===
    let tunnel_stream = if ctx.use_quic {
        connector::connect_quic(&ctx.server_addr, ctx.skip_cert_verify).await?
    } else {
        connector::connect_tcp(&ctx.server_addr).await?
    };

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

    // === Phase 4: Relay data ===
    relay::relay(stream, tunnel_conn).await
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
