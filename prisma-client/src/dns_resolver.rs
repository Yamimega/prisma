//! Client-side DNS resolver that integrates with PrismaVeil DNS modes.
//!
//! - **Direct**: No DNS processing, domains passed to server as-is.
//! - **Tunnel**: All DNS queries sent through the encrypted tunnel via CMD_DNS_QUERY.
//! - **Smart**: Blocked domains resolved via tunnel, others resolved directly.
//! - **Fake**: Assign fake IPs from a reserved pool (used in TUN mode).

use std::net::Ipv4Addr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use tokio::net::UdpSocket;
use tokio::sync::Mutex;

use prisma_core::dns::fake_ip::FakeIpPool;
use prisma_core::dns::{domain_matches_blocklist, DnsConfig, DnsMode};

/// A shared DNS resolver for the client.
#[derive(Clone)]
pub struct DnsResolver {
    inner: Arc<DnsResolverInner>,
}

struct DnsResolverInner {
    mode: DnsMode,
    upstream: String,
    blocklist: Vec<String>,
    fake_ip_pool: Mutex<Option<FakeIpPool>>,
}

impl DnsResolver {
    /// Create a new DNS resolver from config.
    pub fn new(config: &DnsConfig) -> Self {
        let fake_ip_pool = if config.mode == DnsMode::Fake {
            Some(FakeIpPool::new(&config.fake_ip_range))
        } else {
            None
        };

        // For smart mode, we'd load a blocklist from geosite.
        // For now, use a hardcoded set of commonly blocked domains as a starting point.
        let blocklist = if config.mode == DnsMode::Smart {
            default_blocklist()
        } else {
            Vec::new()
        };

        Self {
            inner: Arc::new(DnsResolverInner {
                mode: config.mode.clone(),
                upstream: config.upstream.clone(),
                blocklist,
                fake_ip_pool: Mutex::new(fake_ip_pool),
            }),
        }
    }

    /// Get the DNS mode.
    pub fn mode(&self) -> &DnsMode {
        &self.inner.mode
    }

    /// Check if a domain should be resolved via the tunnel.
    /// Returns true for Tunnel mode (all domains) and Smart mode (blocked domains).
    pub fn should_tunnel_dns(&self, domain: &str) -> bool {
        match self.inner.mode {
            DnsMode::Tunnel => true,
            DnsMode::Smart => domain_matches_blocklist(domain, &self.inner.blocklist),
            DnsMode::Fake | DnsMode::Direct => false,
        }
    }

    /// Resolve a domain directly using the configured upstream DNS.
    /// Used by Smart mode for non-blocked domains and as a fallback.
    pub async fn resolve_direct(&self, domain: &str) -> Result<Vec<Ipv4Addr>> {
        resolve_dns_direct(domain, &self.inner.upstream).await
    }

    /// Assign a fake IP for a domain (Fake DNS mode).
    /// Returns None if not in Fake mode.
    pub async fn assign_fake_ip(&self, domain: &str) -> Option<Ipv4Addr> {
        let mut pool = self.inner.fake_ip_pool.lock().await;
        pool.as_mut().map(|p| p.assign(domain))
    }

    /// Look up the real domain for a fake IP.
    /// Returns None if not in Fake mode or IP not found.
    pub async fn lookup_fake_ip(&self, ip: Ipv4Addr) -> Option<String> {
        let pool = self.inner.fake_ip_pool.lock().await;
        pool.as_ref()
            .and_then(|p| p.lookup(ip).map(|s| s.to_string()))
    }

    /// Check if an IP belongs to the fake IP pool.
    pub async fn is_fake_ip(&self, ip: Ipv4Addr) -> bool {
        let pool = self.inner.fake_ip_pool.lock().await;
        pool.as_ref().map(|p| p.contains(ip)).unwrap_or(false)
    }
}

/// Resolve a domain using direct UDP DNS query to the upstream server.
/// This is a minimal DNS resolver that constructs a raw A record query.
async fn resolve_dns_direct(domain: &str, upstream: &str) -> Result<Vec<Ipv4Addr>> {
    let query = build_dns_query(domain);
    let sock = UdpSocket::bind("0.0.0.0:0").await?;
    sock.send_to(&query, upstream).await?;

    let mut buf = [0u8; 512];
    let (n, _) = tokio::time::timeout(Duration::from_secs(5), sock.recv_from(&mut buf)).await??;

    parse_dns_a_records(&buf[..n])
}

/// Build a minimal DNS A record query for a domain.
fn build_dns_query(domain: &str) -> Vec<u8> {
    let mut buf = Vec::with_capacity(64);

    // Header
    let id: u16 = rand::random();
    buf.extend_from_slice(&id.to_be_bytes()); // ID
    buf.extend_from_slice(&[0x01, 0x00]); // Flags: standard query, recursion desired
    buf.extend_from_slice(&[0x00, 0x01]); // QDCOUNT: 1
    buf.extend_from_slice(&[0x00, 0x00]); // ANCOUNT: 0
    buf.extend_from_slice(&[0x00, 0x00]); // NSCOUNT: 0
    buf.extend_from_slice(&[0x00, 0x00]); // ARCOUNT: 0

    // Question: domain name in DNS wire format
    for label in domain.trim_end_matches('.').split('.') {
        buf.push(label.len() as u8);
        buf.extend_from_slice(label.as_bytes());
    }
    buf.push(0x00); // root label

    buf.extend_from_slice(&[0x00, 0x01]); // QTYPE: A
    buf.extend_from_slice(&[0x00, 0x01]); // QCLASS: IN

    buf
}

/// Parse A records from a DNS response.
fn parse_dns_a_records(data: &[u8]) -> Result<Vec<Ipv4Addr>> {
    if data.len() < 12 {
        return Err(anyhow::anyhow!("DNS response too short"));
    }

    let ancount = u16::from_be_bytes([data[6], data[7]]) as usize;
    let mut addrs = Vec::new();

    // Skip header (12 bytes) and question section
    let mut pos = 12;

    // Skip question section
    while pos < data.len() && data[pos] != 0 {
        if data[pos] & 0xC0 == 0xC0 {
            pos += 2;
            break;
        }
        let len = data[pos] as usize;
        pos += 1 + len;
    }
    if pos < data.len() && data[pos] == 0 {
        pos += 1; // null terminator
    }
    pos += 4; // QTYPE + QCLASS

    // Parse answer records
    for _ in 0..ancount {
        if pos >= data.len() {
            break;
        }

        // Skip name (handle compression)
        if pos < data.len() && data[pos] & 0xC0 == 0xC0 {
            pos += 2;
        } else {
            while pos < data.len() && data[pos] != 0 {
                let len = data[pos] as usize;
                pos += 1 + len;
            }
            pos += 1;
        }

        if pos + 10 > data.len() {
            break;
        }

        let rtype = u16::from_be_bytes([data[pos], data[pos + 1]]);
        let rdlength = u16::from_be_bytes([data[pos + 8], data[pos + 9]]) as usize;
        pos += 10;

        if rtype == 1 && rdlength == 4 && pos + 4 <= data.len() {
            // A record
            addrs.push(Ipv4Addr::new(
                data[pos],
                data[pos + 1],
                data[pos + 2],
                data[pos + 3],
            ));
        }

        pos += rdlength;
    }

    Ok(addrs)
}

/// Build a raw DNS query packet for a domain (for tunneling through CMD_DNS_QUERY).
pub fn build_tunnel_dns_query(domain: &str) -> (u16, Vec<u8>) {
    let query = build_dns_query(domain);
    let id = u16::from_be_bytes([query[0], query[1]]);
    (id, query)
}

/// Parse the first A record IP from a raw DNS response.
pub fn parse_tunnel_dns_response(data: &[u8]) -> Option<Ipv4Addr> {
    parse_dns_a_records(data).ok()?.into_iter().next()
}

/// Default blocklist of commonly blocked domain suffixes (for Smart DNS mode).
/// In production, this would be loaded from a GeoSite database file.
fn default_blocklist() -> Vec<String> {
    vec![
        "google.com".into(),
        "googleapis.com".into(),
        "youtube.com".into(),
        "ytimg.com".into(),
        "googlevideo.com".into(),
        "facebook.com".into(),
        "fbcdn.net".into(),
        "twitter.com".into(),
        "x.com".into(),
        "twimg.com".into(),
        "instagram.com".into(),
        "whatsapp.com".into(),
        "telegram.org".into(),
        "t.me".into(),
        "wikipedia.org".into(),
        "wikimedia.org".into(),
        "github.com".into(),
        "githubusercontent.com".into(),
        "reddit.com".into(),
        "redd.it".into(),
        "medium.com".into(),
        "nytimes.com".into(),
        "bbc.com".into(),
        "bbc.co.uk".into(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_dns_query() {
        let query = build_dns_query("example.com");
        // Header: 12 bytes
        assert_eq!(query[2], 0x01); // flags: RD set
        assert_eq!(query[4], 0x00);
        assert_eq!(query[5], 0x01); // QDCOUNT: 1

        // Question starts at byte 12
        assert_eq!(query[12], 7); // "example" length
        assert_eq!(&query[13..20], b"example");
        assert_eq!(query[20], 3); // "com" length
        assert_eq!(&query[21..24], b"com");
        assert_eq!(query[24], 0); // root label
    }

    #[test]
    fn test_dns_resolver_direct_mode() {
        let config = DnsConfig::default();
        let resolver = DnsResolver::new(&config);
        assert!(!resolver.should_tunnel_dns("google.com"));
        assert!(!resolver.should_tunnel_dns("example.com"));
    }

    #[test]
    fn test_dns_resolver_tunnel_mode() {
        let config = DnsConfig {
            mode: DnsMode::Tunnel,
            ..DnsConfig::default()
        };
        let resolver = DnsResolver::new(&config);
        assert!(resolver.should_tunnel_dns("google.com"));
        assert!(resolver.should_tunnel_dns("example.com"));
    }

    #[test]
    fn test_dns_resolver_smart_mode() {
        let config = DnsConfig {
            mode: DnsMode::Smart,
            ..DnsConfig::default()
        };
        let resolver = DnsResolver::new(&config);
        // Blocked domains should be tunneled
        assert!(resolver.should_tunnel_dns("www.google.com"));
        assert!(resolver.should_tunnel_dns("youtube.com"));
        // Non-blocked domains should not
        assert!(!resolver.should_tunnel_dns("baidu.com"));
        assert!(!resolver.should_tunnel_dns("qq.com"));
    }

    #[tokio::test]
    async fn test_fake_ip_assignment() {
        let config = DnsConfig {
            mode: DnsMode::Fake,
            ..DnsConfig::default()
        };
        let resolver = DnsResolver::new(&config);

        let ip = resolver.assign_fake_ip("google.com").await.unwrap();
        assert!(resolver.is_fake_ip(ip).await);

        let domain = resolver.lookup_fake_ip(ip).await.unwrap();
        assert_eq!(domain, "google.com");

        // Same domain returns same IP
        let ip2 = resolver.assign_fake_ip("google.com").await.unwrap();
        assert_eq!(ip, ip2);
    }
}
