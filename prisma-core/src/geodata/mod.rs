//! GeoIP data loader for v2fly/geoip `.dat` files.
//!
//! Loads protobuf-encoded GeoIP databases and provides fast
//! country-code-based IP matching for routing rules.

mod proto;

use std::collections::HashMap;
use std::net::Ipv4Addr;

use anyhow::{Context, Result};
use prost::Message;
use tracing::info;

/// Pre-parsed GeoIP matcher. Stores IPv4 CIDR ranges grouped by country code.
pub struct GeoIPMatcher {
    /// country_code (lowercase) → Vec<(network_u32, mask_u32)>
    entries: HashMap<String, Vec<(u32, u32)>>,
}

impl GeoIPMatcher {
    /// Load a v2fly geoip.dat file.
    pub fn load(path: &str) -> Result<Self> {
        let data =
            std::fs::read(path).with_context(|| format!("failed to read GeoIP file: {}", path))?;
        let list =
            proto::GeoIPList::decode(data.as_slice()).context("failed to decode GeoIP protobuf")?;

        let mut entries = HashMap::new();
        let mut total_cidrs = 0usize;

        for geo in &list.entry {
            let code = geo.country_code.to_ascii_lowercase();
            let mut cidrs = Vec::with_capacity(geo.cidr.len());
            for cidr in &geo.cidr {
                // Only handle IPv4 (4-byte addresses)
                if cidr.ip.len() == 4 && cidr.prefix <= 32 {
                    let ip_u32 =
                        u32::from_be_bytes([cidr.ip[0], cidr.ip[1], cidr.ip[2], cidr.ip[3]]);
                    let mask = if cidr.prefix == 0 {
                        0
                    } else {
                        !0u32 << (32 - cidr.prefix)
                    };
                    let network = ip_u32 & mask;
                    cidrs.push((network, mask));
                }
            }
            total_cidrs += cidrs.len();
            entries.insert(code, cidrs);
        }

        info!(
            countries = entries.len(),
            cidrs = total_cidrs,
            "Loaded GeoIP database"
        );

        Ok(Self { entries })
    }

    /// Check if an IPv4 address belongs to a given country code.
    pub fn matches(&self, country_code: &str, ip: Ipv4Addr) -> bool {
        let code = country_code.to_ascii_lowercase();
        let ip_u32 = u32::from(ip);
        self.entries
            .get(&code)
            .map(|cidrs| {
                cidrs
                    .iter()
                    .any(|(network, mask)| (ip_u32 & mask) == *network)
            })
            .unwrap_or(false)
    }

    /// Return all loaded country codes.
    pub fn country_codes(&self) -> Vec<&str> {
        self.entries.keys().map(|s| s.as_str()).collect()
    }

    /// Create a matcher from pre-built entries (for testing).
    pub fn new_from_entries(entries: HashMap<String, Vec<(u32, u32)>>) -> Self {
        Self { entries }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matcher_empty() {
        let matcher = GeoIPMatcher {
            entries: HashMap::new(),
        };
        assert!(!matcher.matches("cn", Ipv4Addr::new(1, 2, 3, 4)));
        assert!(matcher.country_codes().is_empty());
    }

    #[test]
    fn test_matcher_basic() {
        let mut entries = HashMap::new();
        // 10.0.0.0/8
        let mask = !0u32 << 24;
        let network = u32::from(Ipv4Addr::new(10, 0, 0, 0)) & mask;
        entries.insert("private".to_string(), vec![(network, mask)]);

        let matcher = GeoIPMatcher { entries };
        assert!(matcher.matches("private", Ipv4Addr::new(10, 1, 2, 3)));
        assert!(!matcher.matches("private", Ipv4Addr::new(192, 168, 1, 1)));
        assert!(!matcher.matches("cn", Ipv4Addr::new(10, 1, 2, 3)));
    }
}
