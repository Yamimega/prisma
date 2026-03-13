//! Local UDP DNS server for TUN mode.
//!
//! Intercepts DNS queries on a configurable local address and handles them
//! according to the configured DNS mode:
//! - **Fake**: Parse the queried domain, assign a fake IP via DnsResolver, return a synthetic A record.
//! - **Tunnel**: Forward the raw DNS query through a PrismaVeil tunnel (CMD_DNS_QUERY),
//!   wait for the response, and relay it back to the client.
//! - **Smart**: Check if the domain should be tunneled; if so, tunnel it, otherwise resolve directly.
//! - **Direct**: Does not start (caller should not invoke this).

use std::net::Ipv4Addr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use tokio::io::AsyncReadExt;
use tokio::net::UdpSocket;
use tracing::{debug, info, warn};

use prisma_core::crypto::aead::{create_cipher, AeadCipher};
use prisma_core::dns::DnsMode;
use prisma_core::protocol::codec::*;
use prisma_core::protocol::types::*;
use prisma_core::types::MAX_FRAME_SIZE;
use prisma_core::util;

use crate::proxy::ProxyContext;
use crate::tunnel;

/// Run the local DNS server. Returns immediately if the mode is Direct.
pub async fn run_dns_server(ctx: ProxyContext) -> Result<()> {
    if *ctx.dns_resolver.mode() == DnsMode::Direct {
        return Ok(());
    }

    let listen_addr = &ctx.dns_config.dns_listen_addr;
    let socket = UdpSocket::bind(listen_addr).await?;
    info!(addr = %listen_addr, mode = ?ctx.dns_resolver.mode(), "DNS server started");

    let socket = Arc::new(socket);

    loop {
        let mut buf = [0u8; 512];
        let (n, src) = match socket.recv_from(&mut buf).await {
            Ok(r) => r,
            Err(e) => {
                warn!("DNS recv error: {}", e);
                continue;
            }
        };

        let query = buf[..n].to_vec();
        let sock = socket.clone();
        let ctx = ctx.clone();

        tokio::spawn(async move {
            match handle_dns_query(&ctx, &query, &sock, src).await {
                Ok(()) => {}
                Err(e) => {
                    debug!("DNS query handler error: {}", e);
                }
            }
        });
    }
}

/// Handle a single DNS query based on the configured mode.
async fn handle_dns_query(
    ctx: &ProxyContext,
    query: &[u8],
    socket: &UdpSocket,
    src: std::net::SocketAddr,
) -> Result<()> {
    match ctx.dns_resolver.mode() {
        DnsMode::Fake => handle_fake(ctx, query, socket, src).await,
        DnsMode::Tunnel => handle_tunnel(ctx, query, socket, src).await,
        DnsMode::Smart => handle_smart(ctx, query, socket, src).await,
        DnsMode::Direct => Ok(()), // should not reach here
    }
}

/// Fake DNS: parse the queried domain, assign a fake IP, return a synthetic A response.
async fn handle_fake(
    ctx: &ProxyContext,
    query: &[u8],
    socket: &UdpSocket,
    src: std::net::SocketAddr,
) -> Result<()> {
    if query.len() < 12 {
        return Err(anyhow::anyhow!("DNS query too short"));
    }

    let domain = parse_domain_from_query(query)?;
    debug!(domain = %domain, "Fake DNS query");

    let ip = ctx
        .dns_resolver
        .assign_fake_ip(&domain)
        .await
        .ok_or_else(|| anyhow::anyhow!("Fake IP pool not available"))?;

    let response = build_a_record_response(query, ip);
    socket.send_to(&response, src).await?;

    debug!(domain = %domain, ip = %ip, "Fake DNS response sent");
    Ok(())
}

/// Tunnel DNS: forward the raw query through a PrismaVeil tunnel, relay the response back.
async fn handle_tunnel(
    ctx: &ProxyContext,
    query: &[u8],
    socket: &UdpSocket,
    src: std::net::SocketAddr,
) -> Result<()> {
    let response = tunnel_dns_query(ctx, query).await?;
    socket.send_to(&response, src).await?;
    debug!("Tunnel DNS response sent");
    Ok(())
}

/// Smart DNS: tunnel if the domain is blocked, resolve directly otherwise.
async fn handle_smart(
    ctx: &ProxyContext,
    query: &[u8],
    socket: &UdpSocket,
    src: std::net::SocketAddr,
) -> Result<()> {
    if query.len() < 12 {
        return Err(anyhow::anyhow!("DNS query too short"));
    }

    let domain = parse_domain_from_query(query)?;
    debug!(domain = %domain, "Smart DNS query");

    if ctx.dns_resolver.should_tunnel_dns(&domain) {
        debug!(domain = %domain, "Smart DNS: tunneling blocked domain");
        let response = tunnel_dns_query(ctx, query).await?;
        socket.send_to(&response, src).await?;
    } else {
        debug!(domain = %domain, "Smart DNS: resolving directly");
        let response = resolve_direct_raw(query, &ctx.dns_config.upstream).await?;
        socket.send_to(&response, src).await?;
    }

    Ok(())
}

/// Forward a raw DNS query through a PrismaVeil tunnel using CMD_DNS_QUERY.
/// Establishes a fresh tunnel, sends the DNS query frame, reads back the DNS response frame.
async fn tunnel_dns_query(ctx: &ProxyContext, query: &[u8]) -> Result<Vec<u8>> {
    let query_id = if query.len() >= 2 {
        u16::from_be_bytes([query[0], query[1]])
    } else {
        0
    };

    // Establish a raw tunnel (handshake + challenge only, no CONNECT command)
    let stream = ctx.connect().await?;
    let tunnel_conn =
        tunnel::establish_raw_tunnel(stream, ctx.client_id, ctx.auth_secret, ctx.cipher_suite)
            .await?;

    let cipher: Arc<dyn AeadCipher> = Arc::from(create_cipher(
        tunnel_conn.session_keys.cipher_suite,
        &tunnel_conn.session_keys.session_key,
    ));
    let (mut tunnel_read, mut tunnel_write) = tokio::io::split(tunnel_conn.stream);
    let mut session_keys = tunnel_conn.session_keys;

    // Send DNS query frame
    let dns_frame = DataFrame {
        command: Command::DnsQuery {
            query_id,
            data: query.to_vec(),
        },
        flags: 0,
        stream_id: 0,
    };
    let frame_bytes = encode_data_frame(&dns_frame);
    let nonce = session_keys.next_client_nonce();
    let encrypted = encrypt_frame(cipher.as_ref(), &nonce, &frame_bytes)?;
    util::write_framed(&mut tunnel_write, &encrypted).await?;

    // Read DNS response frame (with timeout)
    let response = tokio::time::timeout(Duration::from_secs(10), async {
        let mut len_buf = [0u8; 2];
        tunnel_read.read_exact(&mut len_buf).await?;
        let frame_len = u16::from_be_bytes(len_buf) as usize;
        if frame_len > MAX_FRAME_SIZE {
            return Err(anyhow::anyhow!(
                "DNS response frame too large: {}",
                frame_len
            ));
        }
        let mut frame_buf = vec![0u8; frame_len];
        tunnel_read.read_exact(&mut frame_buf).await?;

        let (plaintext, _) = decrypt_frame(cipher.as_ref(), &frame_buf)?;
        let frame = decode_data_frame(&plaintext)?;

        match frame.command {
            Command::DnsResponse { data, .. } => Ok(data),
            _ => Err(anyhow::anyhow!(
                "Expected DnsResponse, got cmd=0x{:02x}",
                frame.command.cmd_byte()
            )),
        }
    })
    .await??;

    Ok(response)
}

/// Resolve a DNS query directly by forwarding the raw packet to the upstream server.
async fn resolve_direct_raw(query: &[u8], upstream: &str) -> Result<Vec<u8>> {
    let sock = UdpSocket::bind("0.0.0.0:0").await?;
    sock.send_to(query, upstream).await?;

    let mut buf = [0u8; 512];
    let (n, _) = tokio::time::timeout(Duration::from_secs(5), sock.recv_from(&mut buf)).await??;

    Ok(buf[..n].to_vec())
}

/// Parse the queried domain name from a DNS query packet.
/// The question section starts at byte 12 (after the 12-byte header).
fn parse_domain_from_query(query: &[u8]) -> Result<String> {
    if query.len() < 13 {
        return Err(anyhow::anyhow!("DNS query too short to contain a question"));
    }

    let mut pos = 12;
    let mut labels = Vec::new();

    loop {
        if pos >= query.len() {
            return Err(anyhow::anyhow!("DNS query truncated in question section"));
        }
        let len = query[pos] as usize;
        if len == 0 {
            break;
        }
        // Pointer compression in the question section is unusual but handle it
        if len & 0xC0 == 0xC0 {
            return Err(anyhow::anyhow!("Unexpected pointer in DNS question"));
        }
        pos += 1;
        if pos + len > query.len() {
            return Err(anyhow::anyhow!("DNS label extends beyond packet"));
        }
        let label = std::str::from_utf8(&query[pos..pos + len])
            .map_err(|_| anyhow::anyhow!("Invalid UTF-8 in DNS label"))?;
        labels.push(label.to_string());
        pos += len;
    }

    if labels.is_empty() {
        return Err(anyhow::anyhow!("Empty domain in DNS query"));
    }

    Ok(labels.join("."))
}

/// Build a DNS A record response for the given query, returning the specified IPv4 address.
///
/// The response mirrors the query's ID and question section, sets response flags,
/// and appends a single A record answer.
fn build_a_record_response(query: &[u8], ip: Ipv4Addr) -> Vec<u8> {
    let mut resp = Vec::with_capacity(query.len() + 16);

    // Copy the transaction ID from the query
    resp.extend_from_slice(&query[..2]);

    // Flags: QR=1 (response), AA=1, RD=1, RA=1
    resp.extend_from_slice(&[0x81, 0x80]);

    // QDCOUNT: 1
    resp.extend_from_slice(&[0x00, 0x01]);
    // ANCOUNT: 1
    resp.extend_from_slice(&[0x00, 0x01]);
    // NSCOUNT: 0
    resp.extend_from_slice(&[0x00, 0x00]);
    // ARCOUNT: 0
    resp.extend_from_slice(&[0x00, 0x00]);

    // Copy the question section from the original query
    // Question starts at byte 12 and goes through the null terminator + QTYPE(2) + QCLASS(2)
    let mut pos = 12;
    while pos < query.len() && query[pos] != 0 {
        let len = query[pos] as usize;
        pos += 1 + len;
    }
    pos += 1; // null terminator
    pos += 4; // QTYPE + QCLASS

    // Sanity check: pos should be within the query
    let question_end = pos.min(query.len());
    resp.extend_from_slice(&query[12..question_end]);

    // Answer section: A record
    // Name: pointer to the domain in the question section (offset 12)
    resp.extend_from_slice(&[0xC0, 0x0C]);
    // TYPE: A (1)
    resp.extend_from_slice(&[0x00, 0x01]);
    // CLASS: IN (1)
    resp.extend_from_slice(&[0x00, 0x01]);
    // TTL: 60 seconds
    resp.extend_from_slice(&[0x00, 0x00, 0x00, 0x3C]);
    // RDLENGTH: 4 (IPv4 address)
    resp.extend_from_slice(&[0x00, 0x04]);
    // RDATA: IPv4 address
    resp.extend_from_slice(&ip.octets());

    resp
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dns_resolver;

    #[test]
    fn test_parse_domain_from_query() {
        // Build a query for "example.com" using the existing helper
        let query = dns_resolver::build_tunnel_dns_query("example.com").1;
        let domain = parse_domain_from_query(&query).unwrap();
        assert_eq!(domain, "example.com");
    }

    #[test]
    fn test_parse_domain_from_query_subdomain() {
        let query = dns_resolver::build_tunnel_dns_query("www.google.com").1;
        let domain = parse_domain_from_query(&query).unwrap();
        assert_eq!(domain, "www.google.com");
    }

    #[test]
    fn test_parse_domain_short_query() {
        let result = parse_domain_from_query(&[0u8; 10]);
        assert!(result.is_err());
    }

    #[test]
    fn test_build_a_record_response() {
        let query = dns_resolver::build_tunnel_dns_query("example.com").1;
        let ip = Ipv4Addr::new(198, 18, 0, 1);
        let resp = build_a_record_response(&query, ip);

        // Transaction ID should match
        assert_eq!(resp[0], query[0]);
        assert_eq!(resp[1], query[1]);

        // Should be a response (QR=1)
        assert_eq!(resp[2] & 0x80, 0x80);

        // QDCOUNT = 1
        assert_eq!(u16::from_be_bytes([resp[4], resp[5]]), 1);
        // ANCOUNT = 1
        assert_eq!(u16::from_be_bytes([resp[6], resp[7]]), 1);

        // The response should end with the IP octets
        let len = resp.len();
        assert_eq!(&resp[len - 4..], &[198, 18, 0, 1]);
    }

    #[test]
    fn test_build_a_record_response_roundtrip() {
        // Verify the response can be parsed by the existing parser
        let query = dns_resolver::build_tunnel_dns_query("test.org").1;
        let ip = Ipv4Addr::new(198, 18, 1, 42);
        let resp = build_a_record_response(&query, ip);

        let parsed = dns_resolver::parse_tunnel_dns_response(&resp);
        assert_eq!(parsed, Some(ip));
    }
}
