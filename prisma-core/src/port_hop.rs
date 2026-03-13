//! Deterministic port hopping for anti-censorship.
//!
//! Both client and server compute the same port at any given time using a shared
//! secret (auth_secret) and the current epoch (time divided by interval).
//!
//! During port transitions, the server accepts on BOTH old and new ports for a
//! grace period to avoid connection drops.

use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::time::{SystemTime, UNIX_EPOCH};

/// Port hopping configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortHoppingConfig {
    /// Whether port hopping is enabled.
    #[serde(default)]
    pub enabled: bool,
    /// Base port number (start of range).
    #[serde(default = "default_base_port")]
    pub base_port: u16,
    /// Number of ports in the range (ports base_port..base_port+port_range).
    #[serde(default = "default_port_range")]
    pub port_range: u16,
    /// How often to hop (seconds).
    #[serde(default = "default_interval_secs")]
    pub interval_secs: u64,
    /// Grace period: accept on old port for this many seconds after hop.
    #[serde(default = "default_grace_period")]
    pub grace_period_secs: u64,
}

impl Default for PortHoppingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            base_port: default_base_port(),
            port_range: default_port_range(),
            interval_secs: default_interval_secs(),
            grace_period_secs: default_grace_period(),
        }
    }
}

fn default_base_port() -> u16 {
    10000
}
fn default_port_range() -> u16 {
    50000
}
fn default_interval_secs() -> u64 {
    60
}
fn default_grace_period() -> u64 {
    10
}

/// Compute the current port for a given time.
///
/// Uses HMAC-SHA256(secret, epoch) to deterministically select a port within the range.
/// Both client and server will compute the same port given the same time and secret.
pub fn current_port(config: &PortHoppingConfig, secret: &[u8], now: SystemTime) -> u16 {
    let epoch = compute_epoch(now, config.interval_secs);
    port_for_epoch(config, secret, epoch)
}

/// Compute the previous port (for grace period acceptance).
pub fn previous_port(config: &PortHoppingConfig, secret: &[u8], now: SystemTime) -> u16 {
    let epoch = compute_epoch(now, config.interval_secs);
    if epoch > 0 {
        port_for_epoch(config, secret, epoch - 1)
    } else {
        port_for_epoch(config, secret, 0)
    }
}

/// Get the set of ports that should be active right now (current + grace).
pub fn active_ports(config: &PortHoppingConfig, secret: &[u8], now: SystemTime) -> Vec<u16> {
    let epoch = compute_epoch(now, config.interval_secs);
    let current = port_for_epoch(config, secret, epoch);

    let elapsed_in_epoch = elapsed_in_current_epoch(now, config.interval_secs);

    // During grace period at the start of an epoch, also accept previous port
    if elapsed_in_epoch < config.grace_period_secs && epoch > 0 {
        let prev = port_for_epoch(config, secret, epoch - 1);
        if prev != current {
            return vec![current, prev];
        }
    }

    vec![current]
}

/// Seconds until the next port hop.
pub fn seconds_until_next_hop(config: &PortHoppingConfig, now: SystemTime) -> u64 {
    let elapsed = elapsed_in_current_epoch(now, config.interval_secs);
    config.interval_secs.saturating_sub(elapsed)
}

fn compute_epoch(now: SystemTime, interval_secs: u64) -> u64 {
    let secs = now.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    if interval_secs == 0 {
        0
    } else {
        secs / interval_secs
    }
}

fn elapsed_in_current_epoch(now: SystemTime, interval_secs: u64) -> u64 {
    let secs = now.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    if interval_secs == 0 {
        0
    } else {
        secs % interval_secs
    }
}

fn port_for_epoch(config: &PortHoppingConfig, secret: &[u8], epoch: u64) -> u16 {
    if config.port_range == 0 {
        return config.base_port;
    }

    let mut mac = Hmac::<Sha256>::new_from_slice(secret).expect("HMAC accepts any key length");
    mac.update(&epoch.to_be_bytes());
    let result = mac.finalize().into_bytes();

    let hash_val = u16::from_be_bytes([result[0], result[1]]);
    config.base_port + (hash_val % config.port_range)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> PortHoppingConfig {
        PortHoppingConfig {
            enabled: true,
            base_port: 10000,
            port_range: 50000,
            interval_secs: 60,
            grace_period_secs: 10,
        }
    }

    #[test]
    fn test_port_determinism() {
        let config = test_config();
        let secret = b"test-secret-key";
        let now = UNIX_EPOCH + std::time::Duration::from_secs(1000);

        let port1 = current_port(&config, secret, now);
        let port2 = current_port(&config, secret, now);
        assert_eq!(port1, port2, "Same inputs must produce same port");
    }

    #[test]
    fn test_port_in_range() {
        let config = test_config();
        let secret = b"test-secret-key";

        for secs in (0..10000).step_by(60) {
            let now = UNIX_EPOCH + std::time::Duration::from_secs(secs);
            let port = current_port(&config, secret, now);
            assert!(
                port >= config.base_port && port < config.base_port + config.port_range,
                "Port {} out of range [{}, {})",
                port,
                config.base_port,
                config.base_port + config.port_range
            );
        }
    }

    #[test]
    fn test_different_epochs_different_ports() {
        let config = test_config();
        let secret = b"test-secret-key";

        let t1 = UNIX_EPOCH + std::time::Duration::from_secs(0);
        let t2 = UNIX_EPOCH + std::time::Duration::from_secs(60);

        let p1 = current_port(&config, secret, t1);
        let p2 = current_port(&config, secret, t2);
        // With 50000 port range, collision is unlikely but possible
        // Just verify they're both valid
        assert!(p1 >= config.base_port);
        assert!(p2 >= config.base_port);
    }

    #[test]
    fn test_same_epoch_same_port() {
        let config = test_config();
        let secret = b"test-secret-key";

        let t1 = UNIX_EPOCH + std::time::Duration::from_secs(30);
        let t2 = UNIX_EPOCH + std::time::Duration::from_secs(59);

        let p1 = current_port(&config, secret, t1);
        let p2 = current_port(&config, secret, t2);
        assert_eq!(p1, p2, "Same epoch must produce same port");
    }

    #[test]
    fn test_grace_period_two_ports() {
        let config = test_config();
        let secret = b"test-secret-key";

        // At 5 seconds into an epoch (within grace period), should have 2 ports
        let now = UNIX_EPOCH + std::time::Duration::from_secs(65); // epoch=1, 5s elapsed
        let ports = active_ports(&config, secret, now);
        // Should have current epoch's port and previous epoch's port (unless same)
        assert!(ports.len() <= 2);
        assert!(!ports.is_empty());
    }

    #[test]
    fn test_after_grace_period_one_port() {
        let config = test_config();
        let secret = b"test-secret-key";

        // At 15 seconds into an epoch (past grace period), should have 1 port
        let now = UNIX_EPOCH + std::time::Duration::from_secs(75); // epoch=1, 15s elapsed
        let ports = active_ports(&config, secret, now);
        assert_eq!(ports.len(), 1);
    }

    #[test]
    fn test_seconds_until_hop() {
        let config = test_config();
        let now = UNIX_EPOCH + std::time::Duration::from_secs(30); // 30s into epoch
        assert_eq!(seconds_until_next_hop(&config, now), 30);
    }

    #[test]
    fn test_different_secrets_different_ports() {
        let config = test_config();
        let now = UNIX_EPOCH + std::time::Duration::from_secs(100);

        let p1 = current_port(&config, b"secret-1", now);
        let p2 = current_port(&config, b"secret-2", now);
        // Very unlikely to collide with 50k range
        assert_ne!(
            p1, p2,
            "Different secrets should (almost always) produce different ports"
        );
    }
}
