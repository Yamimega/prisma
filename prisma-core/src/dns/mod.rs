//! DNS handling for Prisma client.
//!
//! Supports three modes:
//! - **Smart**: Route blocked domain DNS through tunnel, resolve others directly
//! - **Fake**: Return fake IPs from a reserved pool (198.18.0.0/15), map back to real domains
//! - **Tunnel**: Route all DNS queries through the encrypted tunnel

pub mod fake_ip;

use serde::{Deserialize, Serialize};

/// DNS resolution mode.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DnsMode {
    /// Smart DNS: tunnel DNS for blocked domains, resolve others directly.
    Smart,
    /// Fake DNS: return fake IPs from a reserved pool, zero DNS leaks.
    Fake,
    /// Tunnel all DNS through the proxy connection.
    Tunnel,
    /// Direct DNS resolution (no tunneling). Default mode.
    #[default]
    Direct,
}

/// DNS configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsConfig {
    #[serde(default)]
    pub mode: DnsMode,
    /// Path to GeoSite database for smart mode domain matching.
    #[serde(default)]
    pub geosite_path: Option<String>,
    /// CIDR range for fake DNS IPs (default: 198.18.0.0/15).
    #[serde(default = "default_fake_ip_range")]
    pub fake_ip_range: String,
    /// Upstream DNS server for direct resolution.
    #[serde(default = "default_upstream_dns")]
    pub upstream: String,
    /// Local address for the DNS server to listen on (default: 127.0.0.1:53).
    #[serde(default = "default_dns_listen_addr")]
    pub dns_listen_addr: String,
}

impl Default for DnsConfig {
    fn default() -> Self {
        Self {
            mode: DnsMode::default(),
            geosite_path: None,
            fake_ip_range: default_fake_ip_range(),
            upstream: default_upstream_dns(),
            dns_listen_addr: default_dns_listen_addr(),
        }
    }
}

fn default_dns_listen_addr() -> String {
    "127.0.0.1:53".into()
}

fn default_fake_ip_range() -> String {
    "198.18.0.0/15".into()
}

fn default_upstream_dns() -> String {
    "8.8.8.8:53".into()
}

/// Check if a domain matches any entry in a blocklist.
/// Used by smart DNS mode to decide whether to tunnel a query.
pub fn domain_matches_blocklist(domain: &str, blocklist: &[String]) -> bool {
    let domain = domain.trim_end_matches('.');
    for entry in blocklist {
        let entry = entry.trim_start_matches('.');
        if domain == entry || domain.ends_with(&format!(".{}", entry)) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exact_domain_match() {
        let blocklist = vec!["google.com".into()];
        assert!(domain_matches_blocklist("google.com", &blocklist));
        assert!(domain_matches_blocklist("google.com.", &blocklist));
    }

    #[test]
    fn test_subdomain_match() {
        let blocklist = vec!["google.com".into()];
        assert!(domain_matches_blocklist("www.google.com", &blocklist));
        assert!(domain_matches_blocklist("mail.google.com", &blocklist));
        assert!(domain_matches_blocklist("a.b.c.google.com", &blocklist));
    }

    #[test]
    fn test_no_match() {
        let blocklist = vec!["google.com".into()];
        assert!(!domain_matches_blocklist("notgoogle.com", &blocklist));
        assert!(!domain_matches_blocklist("example.com", &blocklist));
    }

    #[test]
    fn test_empty_blocklist() {
        assert!(!domain_matches_blocklist("anything.com", &[]));
    }

    #[test]
    fn test_dns_config_default() {
        let config = DnsConfig::default();
        assert_eq!(config.mode, DnsMode::Direct);
        assert_eq!(config.fake_ip_range, "198.18.0.0/15");
    }
}
