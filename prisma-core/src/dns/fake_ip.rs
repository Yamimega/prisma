//! Fake IP pool for TUN mode DNS.
//!
//! Assigns IPs from a reserved range (198.18.0.0/15 = 131,072 addresses) to domains.
//! When traffic arrives for a fake IP, the real domain is looked up.
//! Uses LRU eviction when the pool is exhausted.

use std::collections::{HashMap, VecDeque};
use std::net::Ipv4Addr;

/// A pool of fake IPs that maps domains to IPs and vice versa.
pub struct FakeIpPool {
    /// Base IP of the pool (network address).
    base: u32,
    /// Number of usable addresses in the pool.
    size: u32,
    /// Next IP offset to allocate.
    next_offset: u32,
    /// Domain → fake IP mapping.
    domain_to_ip: HashMap<String, Ipv4Addr>,
    /// Fake IP → domain mapping.
    ip_to_domain: HashMap<Ipv4Addr, String>,
    /// LRU tracking: front = oldest, back = newest.
    lru_order: VecDeque<String>,
}

impl FakeIpPool {
    /// Create a new FakeIP pool from a CIDR range (e.g., "198.18.0.0/15").
    pub fn new(cidr: &str) -> Self {
        let (base, size) = parse_cidr(cidr);
        Self {
            base,
            size,
            next_offset: 1, // skip network address
            domain_to_ip: HashMap::new(),
            ip_to_domain: HashMap::new(),
            lru_order: VecDeque::new(),
        }
    }

    /// Get or assign a fake IP for the given domain.
    pub fn assign(&mut self, domain: &str) -> Ipv4Addr {
        let domain = domain.trim_end_matches('.').to_lowercase();

        // Already assigned?
        if let Some(&ip) = self.domain_to_ip.get(&domain) {
            self.touch_lru(&domain);
            return ip;
        }

        // Pool exhausted? Evict LRU entry.
        if self.domain_to_ip.len() as u32 >= self.size - 1 {
            self.evict_lru();
        }

        // Assign next IP
        let ip = Ipv4Addr::from(self.base + self.next_offset);
        self.next_offset += 1;
        if self.next_offset >= self.size {
            self.next_offset = 1; // wrap around (skip network addr)
        }

        self.domain_to_ip.insert(domain.clone(), ip);
        self.ip_to_domain.insert(ip, domain.clone());
        self.lru_order.push_back(domain);

        ip
    }

    /// Look up the domain for a fake IP.
    pub fn lookup(&self, ip: Ipv4Addr) -> Option<&str> {
        self.ip_to_domain.get(&ip).map(|s| s.as_str())
    }

    /// Check if an IP is within this pool's range.
    pub fn contains(&self, ip: Ipv4Addr) -> bool {
        let ip_u32 = u32::from(ip);
        ip_u32 >= self.base && ip_u32 < self.base + self.size
    }

    /// Number of currently assigned IPs.
    pub fn len(&self) -> usize {
        self.domain_to_ip.len()
    }

    pub fn is_empty(&self) -> bool {
        self.domain_to_ip.is_empty()
    }

    fn touch_lru(&mut self, domain: &str) {
        if let Some(pos) = self.lru_order.iter().position(|d| d == domain) {
            let d = self.lru_order.remove(pos).unwrap();
            self.lru_order.push_back(d);
        }
    }

    fn evict_lru(&mut self) {
        if let Some(oldest) = self.lru_order.pop_front() {
            if let Some(ip) = self.domain_to_ip.remove(&oldest) {
                self.ip_to_domain.remove(&ip);
            }
        }
    }
}

/// Parse a CIDR string into (base_ip_u32, num_addresses).
fn parse_cidr(cidr: &str) -> (u32, u32) {
    let parts: Vec<&str> = cidr.split('/').collect();
    let ip: Ipv4Addr = parts[0].parse().expect("invalid CIDR IP");
    let prefix_len: u32 = parts[1].parse().expect("invalid CIDR prefix");
    let base = u32::from(ip);
    let size = 1u32 << (32 - prefix_len);
    (base, size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_assign_and_lookup() {
        let mut pool = FakeIpPool::new("198.18.0.0/15");

        let ip = pool.assign("google.com");
        assert!(pool.contains(ip));
        assert_eq!(pool.lookup(ip), Some("google.com"));
    }

    #[test]
    fn test_same_domain_same_ip() {
        let mut pool = FakeIpPool::new("198.18.0.0/15");

        let ip1 = pool.assign("example.com");
        let ip2 = pool.assign("example.com");
        assert_eq!(ip1, ip2);
        assert_eq!(pool.len(), 1);
    }

    #[test]
    fn test_different_domains_different_ips() {
        let mut pool = FakeIpPool::new("198.18.0.0/15");

        let ip1 = pool.assign("google.com");
        let ip2 = pool.assign("facebook.com");
        assert_ne!(ip1, ip2);
        assert_eq!(pool.len(), 2);
    }

    #[test]
    fn test_trailing_dot_stripped() {
        let mut pool = FakeIpPool::new("198.18.0.0/15");

        let ip1 = pool.assign("example.com.");
        let ip2 = pool.assign("example.com");
        assert_eq!(ip1, ip2);
    }

    #[test]
    fn test_case_insensitive() {
        let mut pool = FakeIpPool::new("198.18.0.0/15");

        let ip1 = pool.assign("GOOGLE.COM");
        let ip2 = pool.assign("google.com");
        assert_eq!(ip1, ip2);
    }

    #[test]
    fn test_contains() {
        let pool = FakeIpPool::new("198.18.0.0/15");

        assert!(pool.contains("198.18.0.1".parse().unwrap()));
        assert!(pool.contains("198.19.255.254".parse().unwrap()));
        assert!(!pool.contains("198.20.0.0".parse().unwrap()));
        assert!(!pool.contains("10.0.0.1".parse().unwrap()));
    }

    #[test]
    fn test_small_pool_eviction() {
        // /30 = 4 addresses, 3 usable (skip network addr)
        let mut pool = FakeIpPool::new("10.0.0.0/30");

        let _ip1 = pool.assign("a.com");
        let _ip2 = pool.assign("b.com");
        let _ip3 = pool.assign("c.com");
        assert_eq!(pool.len(), 3);

        // Pool full — assigning new domain should evict oldest (a.com)
        let _ip4 = pool.assign("d.com");
        assert_eq!(pool.len(), 3);
        // a.com was evicted — it should no longer be in the domain map
        assert!(!pool.domain_to_ip.contains_key("a.com"));
        assert!(pool.domain_to_ip.contains_key("d.com"));
    }

    #[test]
    fn test_parse_cidr() {
        let (base, size) = parse_cidr("198.18.0.0/15");
        assert_eq!(base, u32::from(Ipv4Addr::new(198, 18, 0, 0)));
        assert_eq!(size, 131072); // 2^17
    }
}
