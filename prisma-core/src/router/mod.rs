//! Rule-based routing engine for Prisma client.
//!
//! Matches incoming connections against rules and decides:
//! - **Proxy**: send through the PrismaVeil tunnel
//! - **Direct**: connect directly (bypass proxy)
//! - **Block**: drop connection (e.g. ad blocking)

use std::net::IpAddr;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::geodata::GeoIPMatcher;

/// Action to take when a rule matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RouteAction {
    Proxy,
    Direct,
    Block,
}

/// A single routing rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    #[serde(flatten)]
    pub condition: RuleCondition,
    pub action: RouteAction,
}

/// Rule matching conditions.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum RuleCondition {
    /// Exact domain match.
    #[serde(rename = "domain")]
    Domain(String),
    /// Domain suffix match (e.g., ".google.com" matches "www.google.com").
    #[serde(rename = "domain-suffix")]
    DomainSuffix(String),
    /// Domain contains keyword.
    #[serde(rename = "domain-keyword")]
    DomainKeyword(String),
    /// IP CIDR range match.
    #[serde(rename = "ip-cidr")]
    IpCidr(String),
    /// GeoIP country match (e.g., "cn", "us", "private").
    #[serde(rename = "geoip")]
    GeoIp(String),
    /// Port number or range (e.g., "80" or "8000-9000").
    #[serde(rename = "port")]
    Port(String),
    /// Match all connections (catch-all).
    #[serde(rename = "all")]
    All,
}

/// The routing engine: evaluates rules top-to-bottom, first match wins.
pub struct Router {
    rules: Vec<Rule>,
    /// Pre-parsed CIDR entries for fast IP matching.
    cidrs: Vec<(u32, u32)>, // (network_u32, mask_u32) per IpCidr rule index
    /// Optional GeoIP matcher for country-based routing.
    geoip: Option<Arc<GeoIPMatcher>>,
}

impl Router {
    /// Create a router from a list of rules.
    pub fn new(rules: Vec<Rule>) -> Self {
        Self::with_geoip(rules, None)
    }

    /// Create a router with an optional GeoIP matcher.
    pub fn with_geoip(rules: Vec<Rule>, geoip: Option<Arc<GeoIPMatcher>>) -> Self {
        let cidrs = rules
            .iter()
            .map(|r| {
                if let RuleCondition::IpCidr(ref cidr) = r.condition {
                    parse_cidr_v4(cidr).unwrap_or((0, 0))
                } else {
                    (0, 0)
                }
            })
            .collect();
        Self {
            rules,
            cidrs,
            geoip,
        }
    }

    /// Match a connection by domain, IP, and port.
    /// Returns the action for the first matching rule, or `Proxy` if no match.
    pub fn route(&self, domain: Option<&str>, ip: Option<IpAddr>, port: u16) -> RouteAction {
        for (i, rule) in self.rules.iter().enumerate() {
            if self.matches(i, &rule.condition, domain, ip, port) {
                return rule.action;
            }
        }
        RouteAction::Proxy // default: tunnel everything
    }

    fn matches(
        &self,
        idx: usize,
        condition: &RuleCondition,
        domain: Option<&str>,
        ip: Option<IpAddr>,
        port: u16,
    ) -> bool {
        match condition {
            RuleCondition::Domain(d) => {
                domain.is_some_and(|dom| dom.trim_end_matches('.').eq_ignore_ascii_case(d))
            }
            RuleCondition::DomainSuffix(suffix) => domain.is_some_and(|dom| {
                let dom = dom.trim_end_matches('.');
                let suffix = suffix.trim_start_matches('.');
                dom.eq_ignore_ascii_case(suffix)
                    || dom
                        .to_ascii_lowercase()
                        .ends_with(&format!(".{}", suffix.to_ascii_lowercase()))
            }),
            RuleCondition::DomainKeyword(kw) => domain
                .is_some_and(|dom| dom.to_ascii_lowercase().contains(&kw.to_ascii_lowercase())),
            RuleCondition::IpCidr(_) => {
                if let Some(IpAddr::V4(v4)) = ip {
                    let (network, mask) = self.cidrs[idx];
                    let ip_u32 = u32::from(v4);
                    (ip_u32 & mask) == network
                } else {
                    false // TODO: IPv6 CIDR support
                }
            }
            RuleCondition::GeoIp(code) => {
                if let (Some(geoip), Some(IpAddr::V4(v4))) = (&self.geoip, ip) {
                    geoip.matches(code, v4)
                } else {
                    false
                }
            }
            RuleCondition::Port(p) => parse_port_match(p, port),
            RuleCondition::All => true,
        }
    }
}

/// Parse a CIDR string into (network_u32, mask_u32).
pub fn parse_cidr_v4(cidr: &str) -> Option<(u32, u32)> {
    let parts: Vec<&str> = cidr.split('/').collect();
    if parts.len() != 2 {
        return None;
    }
    let ip: std::net::Ipv4Addr = parts[0].parse().ok()?;
    let prefix: u32 = parts[1].parse().ok()?;
    if prefix > 32 {
        return None;
    }
    let mask = if prefix == 0 {
        0
    } else {
        !0u32 << (32 - prefix)
    };
    let network = u32::from(ip) & mask;
    Some((network, mask))
}

/// Match a port against a port spec (single port or range).
fn parse_port_match(spec: &str, port: u16) -> bool {
    if let Some((start, end)) = spec.split_once('-') {
        let start: u16 = start.parse().unwrap_or(0);
        let end: u16 = end.parse().unwrap_or(0);
        port >= start && port <= end
    } else {
        spec.parse::<u16>() == Ok(port)
    }
}

/// Routing configuration for config files.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RoutingConfig {
    #[serde(default)]
    pub rules: Vec<Rule>,
    /// Path to a v2fly geoip.dat file for GeoIP-based routing.
    #[serde(default)]
    pub geoip_path: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_domain_exact_match() {
        let rules = vec![Rule {
            condition: RuleCondition::Domain("example.com".into()),
            action: RouteAction::Direct,
        }];
        let router = Router::new(rules);

        assert_eq!(
            router.route(Some("example.com"), None, 443),
            RouteAction::Direct
        );
        assert_eq!(
            router.route(Some("example.com."), None, 443),
            RouteAction::Direct
        );
        assert_eq!(
            router.route(Some("other.com"), None, 443),
            RouteAction::Proxy
        );
    }

    #[test]
    fn test_domain_suffix_match() {
        let rules = vec![Rule {
            condition: RuleCondition::DomainSuffix("google.com".into()),
            action: RouteAction::Direct,
        }];
        let router = Router::new(rules);

        assert_eq!(
            router.route(Some("google.com"), None, 80),
            RouteAction::Direct
        );
        assert_eq!(
            router.route(Some("www.google.com"), None, 80),
            RouteAction::Direct
        );
        assert_eq!(
            router.route(Some("mail.google.com"), None, 80),
            RouteAction::Direct
        );
        assert_eq!(
            router.route(Some("notgoogle.com"), None, 80),
            RouteAction::Proxy
        );
    }

    #[test]
    fn test_domain_keyword_match() {
        let rules = vec![Rule {
            condition: RuleCondition::DomainKeyword("ads".into()),
            action: RouteAction::Block,
        }];
        let router = Router::new(rules);

        assert_eq!(
            router.route(Some("ads.example.com"), None, 80),
            RouteAction::Block
        );
        assert_eq!(
            router.route(Some("cdn-ads.tracker.com"), None, 80),
            RouteAction::Block
        );
        assert_eq!(
            router.route(Some("example.com"), None, 80),
            RouteAction::Proxy
        );
    }

    #[test]
    fn test_ip_cidr_match() {
        let rules = vec![Rule {
            condition: RuleCondition::IpCidr("192.168.0.0/16".into()),
            action: RouteAction::Direct,
        }];
        let router = Router::new(rules);

        let local_ip = "192.168.1.100".parse().ok();
        let public_ip = "8.8.8.8".parse().ok();

        assert_eq!(router.route(None, local_ip, 80), RouteAction::Direct);
        assert_eq!(router.route(None, public_ip, 80), RouteAction::Proxy);
    }

    #[test]
    fn test_geoip_no_matcher() {
        let rules = vec![Rule {
            condition: RuleCondition::GeoIp("cn".into()),
            action: RouteAction::Direct,
        }];
        let router = Router::new(rules);

        // Without a GeoIP matcher, GeoIp rules never match
        let ip = "1.2.3.4".parse().ok();
        assert_eq!(router.route(None, ip, 80), RouteAction::Proxy);
    }

    #[test]
    fn test_geoip_with_matcher() {
        use std::collections::HashMap;
        use std::net::Ipv4Addr;

        // Build a mock matcher with 10.0.0.0/8 as "private"
        let mut entries = HashMap::new();
        let mask = !0u32 << 24;
        let network = u32::from(Ipv4Addr::new(10, 0, 0, 0)) & mask;
        entries.insert("private".to_string(), vec![(network, mask)]);
        let matcher = Arc::new(GeoIPMatcher::new_from_entries(entries));

        let rules = vec![
            Rule {
                condition: RuleCondition::GeoIp("private".into()),
                action: RouteAction::Direct,
            },
            Rule {
                condition: RuleCondition::All,
                action: RouteAction::Proxy,
            },
        ];
        let router = Router::with_geoip(rules, Some(matcher));

        let private_ip: Option<IpAddr> = "10.1.2.3".parse().ok();
        let public_ip: Option<IpAddr> = "8.8.8.8".parse().ok();

        assert_eq!(router.route(None, private_ip, 80), RouteAction::Direct);
        assert_eq!(router.route(None, public_ip, 80), RouteAction::Proxy);
    }

    #[test]
    fn test_port_match() {
        let rules = vec![
            Rule {
                condition: RuleCondition::Port("80".into()),
                action: RouteAction::Direct,
            },
            Rule {
                condition: RuleCondition::Port("8000-9000".into()),
                action: RouteAction::Block,
            },
        ];
        let router = Router::new(rules);

        assert_eq!(router.route(None, None, 80), RouteAction::Direct);
        assert_eq!(router.route(None, None, 8500), RouteAction::Block);
        assert_eq!(router.route(None, None, 443), RouteAction::Proxy);
    }

    #[test]
    fn test_all_catch_all() {
        let rules = vec![
            Rule {
                condition: RuleCondition::Domain("special.com".into()),
                action: RouteAction::Direct,
            },
            Rule {
                condition: RuleCondition::All,
                action: RouteAction::Block,
            },
        ];
        let router = Router::new(rules);

        assert_eq!(
            router.route(Some("special.com"), None, 80),
            RouteAction::Direct
        );
        assert_eq!(
            router.route(Some("anything.com"), None, 80),
            RouteAction::Block
        );
    }

    #[test]
    fn test_first_match_wins() {
        let rules = vec![
            Rule {
                condition: RuleCondition::Domain("example.com".into()),
                action: RouteAction::Direct,
            },
            Rule {
                condition: RuleCondition::DomainSuffix("example.com".into()),
                action: RouteAction::Block,
            },
        ];
        let router = Router::new(rules);

        // First rule matches exact domain
        assert_eq!(
            router.route(Some("example.com"), None, 80),
            RouteAction::Direct
        );
        // Second rule matches subdomain
        assert_eq!(
            router.route(Some("sub.example.com"), None, 80),
            RouteAction::Block
        );
    }

    #[test]
    fn test_default_proxy() {
        let router = Router::new(vec![]);
        assert_eq!(
            router.route(Some("anything.com"), None, 443),
            RouteAction::Proxy
        );
    }

    #[test]
    fn test_parse_port_match() {
        assert!(parse_port_match("80", 80));
        assert!(!parse_port_match("80", 443));
        assert!(parse_port_match("8000-9000", 8500));
        assert!(!parse_port_match("8000-9000", 7999));
        assert!(parse_port_match("8000-9000", 8000));
        assert!(parse_port_match("8000-9000", 9000));
    }
}
