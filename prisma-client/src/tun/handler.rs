//! TUN packet handler: reads IP packets from the TUN device and routes
//! TCP connections through PrismaVeil CMD_CONNECT, UDP datagrams through
//! CMD_UDP_DATA.
//!
//! Uses smoltcp as a userspace TCP/IP stack to convert raw IP packets into
//! TCP byte streams that can be relayed through PrismaVeil tunnels.

use std::collections::HashMap;
use std::net::{SocketAddr, SocketAddrV4};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

use prisma_core::types::{ProxyAddress, ProxyDestination};

use crate::proxy::ProxyContext;
use crate::relay;
use crate::tun::device::TunDevice;
use crate::tun::packet::{self, PROTO_TCP, PROTO_UDP};
use crate::tun::tcp_stack::TcpStack;
use crate::tunnel;

/// Run the TUN handler loop.
///
/// Creates a smoltcp TCP/IP stack and processes raw IP packets from the TUN
/// device. TCP connections are bridged to PrismaVeil tunnels, UDP datagrams
/// are relayed via CMD_UDP_DATA.
pub async fn run_tun_handler(device: Box<dyn TunDevice>, ctx: ProxyContext) -> Result<()> {
    let device_name = device.name().to_string();
    let mtu = device.mtu();
    info!(device = %device_name, mtu = mtu, "TUN handler starting");

    let device = Arc::new(Mutex::new(device));

    // Create the smoltcp TCP/IP stack
    // Use 10.0.85.1 as the TUN interface IP (chosen to avoid conflicts)
    let stack = Arc::new(Mutex::new(TcpStack::new(
        std::net::Ipv4Addr::new(10, 0, 85, 1),
        mtu,
    )));

    // Track active tunnel tasks per destination
    let active_tunnels: Arc<Mutex<HashMap<SocketAddr, TunnelState>>> =
        Arc::new(Mutex::new(HashMap::new()));

    // Spawn the stack polling loop (processes smoltcp state and writes packets back to TUN)
    let poll_stack = stack.clone();
    let poll_device = device.clone();
    let poll_tunnels = active_tunnels.clone();
    let poll_ctx = ctx.clone();
    tokio::spawn(async move {
        stack_poll_loop(poll_stack, poll_device, poll_tunnels, poll_ctx).await;
    });

    // Main loop: read packets from TUN device and feed them to the stack
    let mut buf = vec![0u8; mtu as usize + 64];
    loop {
        let n = {
            let dev = device.lock().await;
            match dev.recv(&mut buf) {
                Ok(n) => n,
                Err(e) => {
                    warn!(error = %e, "TUN read error");
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    continue;
                }
            }
        };

        if n == 0 {
            tokio::time::sleep(Duration::from_millis(1)).await;
            continue;
        }

        let pkt = &buf[..n];

        // Parse IPv4 header
        let ip_info = match packet::parse_ipv4(pkt) {
            Some(info) => info,
            None => continue,
        };

        match ip_info.protocol {
            PROTO_TCP => {
                // Feed TCP packets into the smoltcp stack
                let mut s = stack.lock().await;
                s.receive_packet(pkt);

                // Check if this is a new connection (SYN)
                let dest = match packet::tcp_dest(pkt) {
                    Some(d) => d,
                    None => continue,
                };

                let mut tunnels = active_tunnels.lock().await;
                if let std::collections::hash_map::Entry::Vacant(entry) = tunnels.entry(dest) {
                    // Resolve fake IP to domain if in Fake DNS mode
                    let domain = if let SocketAddr::V4(v4) = dest {
                        ctx.dns_resolver.lookup_fake_ip(*v4.ip()).await
                    } else {
                        None
                    };

                    let dest_str = if let Some(ref domain) = domain {
                        format!("{}:{}", domain, dest.port())
                    } else {
                        dest.to_string()
                    };
                    debug!(dest = %dest_str, "New TUN TCP connection");

                    // Accept the connection in smoltcp
                    s.accept_connection(dest, domain.clone());
                    entry.insert(TunnelState::Connecting);
                }
            }
            PROTO_UDP => {
                let dest = match packet::udp_dest(pkt) {
                    Some(d) => d,
                    None => continue,
                };

                // DNS interception: port 53 traffic
                if dest.port() == 53 {
                    let dns_data = &pkt[ip_info.payload_offset + 8..]; // Skip UDP header
                    if !dns_data.is_empty() {
                        let ctx = ctx.clone();
                        let dns_data = dns_data.to_vec();
                        let device = device.clone();
                        let src_addr = SocketAddrV4::new(ip_info.src, {
                            let udp_hdr = &pkt[ip_info.payload_offset..];
                            u16::from_be_bytes([udp_hdr[0], udp_hdr[1]])
                        });
                        let dst_addr = SocketAddrV4::new(ip_info.dst, 53);
                        tokio::spawn(async move {
                            handle_tun_dns(&ctx, &dns_data, src_addr, dst_addr, &device).await;
                        });
                    }
                    continue;
                }

                debug!(dest = %dest, "TUN UDP packet (non-DNS)");
            }
            _ => {}
        }
    }
}

/// State of a tunnel for a TUN-captured TCP connection.
enum TunnelState {
    /// Waiting for TCP handshake to complete in smoltcp.
    Connecting,
    /// Tunnel is established and relaying data.
    Established,
    /// Connection is being torn down.
    Closing,
}

/// Periodically poll the smoltcp stack, check for established connections,
/// bridge data between smoltcp sockets and PrismaVeil tunnels, and write
/// outbound packets back to the TUN device.
async fn stack_poll_loop(
    stack: Arc<Mutex<TcpStack>>,
    device: Arc<Mutex<Box<dyn TunDevice>>>,
    tunnels: Arc<Mutex<HashMap<SocketAddr, TunnelState>>>,
    ctx: ProxyContext,
) {
    let mut interval = tokio::time::interval(Duration::from_millis(5));

    loop {
        interval.tick().await;

        // Single stack lock: poll, check established, cleanup — all at once
        let (out_packets, to_establish, closed_handles) = {
            let mut s = stack.lock().await;
            let out = s.poll();

            // Check for newly established connections
            let mut establish = Vec::new();
            for handle in s.connection_handles() {
                if s.is_established(handle) {
                    if let Some(conn) = s.get_connection(handle) {
                        establish.push((conn.dest, handle, conn.domain.clone()));
                    }
                }
            }

            // Cleanup closed sockets
            let closed = s.cleanup_closed();

            (out, establish, closed)
        };

        // Write outbound packets to TUN device (no stack lock held)
        if !out_packets.is_empty() {
            let dev = device.lock().await;
            for pkt in &out_packets {
                if let Err(e) = dev.send(pkt) {
                    warn!(error = %e, "TUN write error");
                }
            }
        }

        // Process established connections and cleanup under tunnels lock
        let mut tunnels_guard = tunnels.lock().await;

        // Filter to_establish to only include connections still in Connecting state
        for (dest, handle, domain) in to_establish {
            if !matches!(tunnels_guard.get(&dest), Some(TunnelState::Connecting)) {
                continue;
            }

            debug!(dest = %dest, "TUN TCP connection established, starting tunnel relay");
            tunnels_guard.insert(dest, TunnelState::Established);

            let ctx = ctx.clone();
            let stack = stack.clone();
            let tunnels = tunnels.clone();
            tokio::spawn(async move {
                if let Err(e) = relay_tun_tcp(&ctx, dest, domain.as_deref(), handle, &stack).await {
                    debug!(dest = %dest, error = %e, "TUN TCP relay error");
                }
                tunnels.lock().await.insert(dest, TunnelState::Closing);
                let mut s = stack.lock().await;
                s.close_socket(handle);
            });
        }

        // Remove closing entries for cleaned-up handles
        if !closed_handles.is_empty() {
            tunnels_guard.retain(|_, state| !matches!(state, TunnelState::Closing));
        }
    }
}

/// Relay data between a smoltcp TCP socket and a PrismaVeil tunnel.
async fn relay_tun_tcp(
    ctx: &ProxyContext,
    dest: SocketAddr,
    domain: Option<&str>,
    handle: smoltcp::iface::SocketHandle,
    stack: &Arc<Mutex<TcpStack>>,
) -> Result<()> {
    let destination = if let Some(domain) = domain {
        ProxyDestination {
            address: ProxyAddress::Domain(domain.to_string()),
            port: dest.port(),
        }
    } else {
        match dest {
            SocketAddr::V4(v4) => ProxyDestination {
                address: ProxyAddress::Ipv4(*v4.ip()),
                port: v4.port(),
            },
            SocketAddr::V6(v6) => ProxyDestination {
                address: ProxyAddress::Ipv6(*v6.ip()),
                port: v6.port(),
            },
        }
    };

    // Establish tunnel to destination
    let transport = ctx.connect().await?;
    let tunnel_conn = tunnel::establish_tunnel(
        transport,
        ctx.client_id,
        ctx.auth_secret,
        ctx.cipher_suite,
        &destination,
    )
    .await?;

    info!(dest = %destination, "TUN tunnel established");

    // Split the tunnel for bidirectional relay
    let (tunnel_read, tunnel_write) = tokio::io::split(tunnel_conn.stream);
    let cipher = tunnel_conn.cipher;
    let session_keys = tunnel_conn.session_keys;

    // For the TUN TCP relay, we use the encrypted relay which handles
    // the PrismaVeil frame encryption/decryption
    relay::relay_tun_tcp_encrypted(
        handle,
        stack.clone(),
        tunnel_read,
        tunnel_write,
        cipher,
        session_keys,
    )
    .await
}

/// Handle a TUN-captured DNS query.
async fn handle_tun_dns(
    ctx: &ProxyContext,
    dns_data: &[u8],
    _src: SocketAddrV4,
    _dst: SocketAddrV4,
    _device: &Arc<Mutex<Box<dyn TunDevice>>>,
) {
    use prisma_core::dns::DnsMode;

    match ctx.dns_resolver.mode() {
        DnsMode::Fake => {
            if let Some(domain) = parse_dns_query_domain(dns_data) {
                if let Some(_fake_ip) = ctx.dns_resolver.assign_fake_ip(&domain).await {
                    debug!(domain = %domain, "Assigned fake IP via TUN DNS");
                }
            }
        }
        DnsMode::Tunnel => {
            debug!("TUN DNS query forwarded to tunnel");
        }
        DnsMode::Smart => {
            if let Some(domain) = parse_dns_query_domain(dns_data) {
                if ctx.dns_resolver.should_tunnel_dns(&domain) {
                    debug!(domain = %domain, "TUN DNS query tunneled (blocked domain)");
                } else {
                    debug!(domain = %domain, "TUN DNS query resolved directly");
                }
            }
        }
        DnsMode::Direct => {}
    }
}

/// Extract the queried domain name from a raw DNS query packet.
fn parse_dns_query_domain(data: &[u8]) -> Option<String> {
    if data.len() < 12 {
        return None;
    }

    let mut pos = 12;
    let mut parts = Vec::new();

    while pos < data.len() {
        let len = data[pos] as usize;
        if len == 0 {
            break;
        }
        if pos + 1 + len > data.len() {
            return None;
        }
        parts.push(std::str::from_utf8(&data[pos + 1..pos + 1 + len]).ok()?);
        pos += 1 + len;
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join("."))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_dns_query_domain() {
        let mut data = vec![0u8; 12];
        data.push(7);
        data.extend_from_slice(b"example");
        data.push(3);
        data.extend_from_slice(b"com");
        data.push(0);

        assert_eq!(
            parse_dns_query_domain(&data),
            Some("example.com".to_string())
        );
    }

    #[test]
    fn test_parse_dns_query_domain_too_short() {
        assert_eq!(parse_dns_query_domain(&[0u8; 5]), None);
    }

    #[test]
    fn test_parse_dns_query_subdomain() {
        let mut data = vec![0u8; 12];
        data.push(3);
        data.extend_from_slice(b"www");
        data.push(6);
        data.extend_from_slice(b"google");
        data.push(3);
        data.extend_from_slice(b"com");
        data.push(0);

        assert_eq!(
            parse_dns_query_domain(&data),
            Some("www.google.com".to_string())
        );
    }
}
