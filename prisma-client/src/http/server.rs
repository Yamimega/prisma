use anyhow::Result;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tracing::{debug, info, warn};

use prisma_core::types::{ProxyAddress, ProxyDestination};

use crate::proxy::ProxyContext;
use crate::relay;
use crate::tunnel;

/// HTTP proxy server supporting the CONNECT method for tunneling.
pub async fn run_http_proxy(listen_addr: &str, ctx: ProxyContext) -> Result<()> {
    let listener = TcpListener::bind(listen_addr).await?;
    info!(addr = %listen_addr, "HTTP proxy server started");

    loop {
        match listener.accept().await {
            Ok((stream, peer)) => {
                let ctx = ctx.clone();
                debug!(peer = %peer, "HTTP proxy client connected");
                tokio::spawn(async move {
                    if let Err(e) = handle_http_client(stream, &ctx).await {
                        warn!(peer = %peer, error = %e, "HTTP proxy session error");
                    }
                });
            }
            Err(e) => {
                warn!(error = %e, "Failed to accept HTTP proxy connection");
            }
        }
    }
}

async fn handle_http_client(stream: TcpStream, ctx: &ProxyContext) -> Result<()> {
    let mut buf_reader = BufReader::new(stream);

    // Read the request line: "CONNECT host:port HTTP/1.1\r\n"
    let mut request_line = String::new();
    buf_reader.read_line(&mut request_line).await?;
    let request_line = request_line.trim_end();

    let parts: Vec<&str> = request_line.split_whitespace().collect();
    if parts.len() < 3 {
        let mut stream = buf_reader.into_inner();
        send_http_error(&mut stream, 400, "Bad Request").await?;
        return Err(anyhow::anyhow!("Malformed request line: {}", request_line));
    }

    let method = parts[0];
    let target = parts[1];

    if !method.eq_ignore_ascii_case("CONNECT") {
        let mut stream = buf_reader.into_inner();
        send_http_error(&mut stream, 405, "Method Not Allowed").await?;
        return Err(anyhow::anyhow!(
            "Only CONNECT is supported, got: {}",
            method
        ));
    }

    // Parse host:port from CONNECT target
    let destination = parse_connect_target(target)?;

    // Consume remaining headers until empty line
    loop {
        let mut header = String::new();
        buf_reader.read_line(&mut header).await?;
        if header.trim().is_empty() {
            break;
        }
    }

    info!(dest = %destination, "HTTP CONNECT");

    // Establish tunnel to remote Prisma server
    let tunnel_stream = ctx.connect().await?;

    let tunnel_conn = tunnel::establish_tunnel(
        tunnel_stream,
        ctx.client_id,
        ctx.auth_secret,
        ctx.cipher_suite,
        &destination,
    )
    .await?;

    // Send 200 Connection Established
    let mut stream = buf_reader.into_inner();
    stream
        .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
        .await?;

    // Relay data bidirectionally
    relay::relay(stream, tunnel_conn).await
}

/// Parse a CONNECT target like "example.com:443" or "[::1]:443" or "1.2.3.4:80"
fn parse_connect_target(target: &str) -> Result<ProxyDestination> {
    // Handle IPv6 bracket notation: [::1]:port
    if let Some(rest) = target.strip_prefix('[') {
        let (addr_str, port_str) = rest
            .split_once("]:")
            .ok_or_else(|| anyhow::anyhow!("Invalid IPv6 CONNECT target: {}", target))?;
        let port: u16 = port_str.parse()?;
        let addr: std::net::Ipv6Addr = addr_str.parse()?;
        return Ok(ProxyDestination {
            address: ProxyAddress::Ipv6(addr),
            port,
        });
    }

    // Split on last ':' to separate host from port
    let (host, port_str) = target
        .rsplit_once(':')
        .ok_or_else(|| anyhow::anyhow!("Missing port in CONNECT target: {}", target))?;
    let port: u16 = port_str
        .parse()
        .map_err(|_| anyhow::anyhow!("Invalid port in CONNECT target: {}", target))?;

    // Try parsing as IPv4, otherwise treat as domain
    if let Ok(ipv4) = host.parse::<std::net::Ipv4Addr>() {
        Ok(ProxyDestination {
            address: ProxyAddress::Ipv4(ipv4),
            port,
        })
    } else {
        Ok(ProxyDestination {
            address: ProxyAddress::Domain(host.to_string()),
            port,
        })
    }
}

async fn send_http_error(stream: &mut TcpStream, code: u16, reason: &str) -> Result<()> {
    let response = format!(
        "HTTP/1.1 {} {}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        code, reason
    );
    stream.write_all(response.as_bytes()).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_domain_target() {
        let dest = parse_connect_target("example.com:443").unwrap();
        assert_eq!(dest.address, ProxyAddress::Domain("example.com".into()));
        assert_eq!(dest.port, 443);
    }

    #[test]
    fn test_parse_ipv4_target() {
        let dest = parse_connect_target("1.2.3.4:8080").unwrap();
        assert_eq!(
            dest.address,
            ProxyAddress::Ipv4("1.2.3.4".parse().unwrap())
        );
        assert_eq!(dest.port, 8080);
    }

    #[test]
    fn test_parse_ipv6_target() {
        let dest = parse_connect_target("[::1]:443").unwrap();
        assert_eq!(dest.address, ProxyAddress::Ipv6("::1".parse().unwrap()));
        assert_eq!(dest.port, 443);
    }

    #[test]
    fn test_parse_missing_port() {
        assert!(parse_connect_target("example.com").is_err());
    }

    #[test]
    fn test_parse_invalid_port() {
        assert!(parse_connect_target("example.com:notaport").is_err());
    }
}
