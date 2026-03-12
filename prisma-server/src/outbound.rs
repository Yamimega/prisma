use anyhow::Result;
use std::net::SocketAddr;
use tokio::net::TcpStream;
use tracing::debug;

use prisma_core::cache::DnsCache;
use prisma_core::types::{ProxyAddress, ProxyDestination};

/// Connect to the target destination via TCP, using the DNS cache for domain lookups.
pub async fn connect(dest: &ProxyDestination, dns_cache: &DnsCache) -> Result<TcpStream> {
    let stream = match &dest.address {
        ProxyAddress::Ipv4(ip) => {
            let sock: SocketAddr = (*ip, dest.port).into();
            debug!(target = %sock, "Opening outbound connection (IPv4)");
            TcpStream::connect(sock).await?
        }
        ProxyAddress::Ipv6(ip) => {
            let sock: SocketAddr = (*ip, dest.port).into();
            debug!(target = %sock, "Opening outbound connection (IPv6)");
            TcpStream::connect(sock).await?
        }
        ProxyAddress::Domain(domain) => {
            debug!(domain = %domain, port = dest.port, "Resolving domain via DNS cache");
            let ips = dns_cache.resolve(domain).await?;
            let ip = ips
                .first()
                .ok_or_else(|| anyhow::anyhow!("DNS resolution returned no addresses for {}", domain))?;
            let sock: SocketAddr = (*ip, dest.port).into();
            debug!(target = %sock, domain = %domain, "Opening outbound connection (domain)");
            TcpStream::connect(sock).await?
        }
    };

    debug!(peer = %stream.peer_addr()?, "Outbound connection established");
    Ok(stream)
}
