//! Traffic quota tracking per client.
//!
//! Tracks bytes uploaded/downloaded per client with configurable reset periods.
//! Uses in-memory counters with optional persistence.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

/// Quota configuration for a client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaConfig {
    /// Total quota in bytes (0 = unlimited).
    pub quota_bytes: u64,
    /// Reset period.
    #[serde(default = "default_period")]
    pub period: QuotaPeriod,
}

impl Default for QuotaConfig {
    fn default() -> Self {
        Self {
            quota_bytes: 0,
            period: QuotaPeriod::Monthly,
        }
    }
}

fn default_period() -> QuotaPeriod {
    QuotaPeriod::Monthly
}

/// Quota reset period.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QuotaPeriod {
    Daily,
    Weekly,
    Monthly,
}

/// Per-client usage counters.
pub struct ClientUsage {
    pub bytes_up: AtomicU64,
    pub bytes_down: AtomicU64,
    pub quota_bytes: u64,
}

impl ClientUsage {
    pub fn new(quota_bytes: u64) -> Self {
        Self {
            bytes_up: AtomicU64::new(0),
            bytes_down: AtomicU64::new(0),
            quota_bytes,
        }
    }

    /// Record uploaded bytes.
    pub fn add_upload(&self, bytes: u64) {
        self.bytes_up.fetch_add(bytes, Ordering::Relaxed);
    }

    /// Record downloaded bytes.
    pub fn add_download(&self, bytes: u64) {
        self.bytes_down.fetch_add(bytes, Ordering::Relaxed);
    }

    /// Total bytes used (up + down).
    pub fn total(&self) -> u64 {
        self.bytes_up.load(Ordering::Relaxed) + self.bytes_down.load(Ordering::Relaxed)
    }

    /// Check if quota is exceeded (0 = unlimited).
    pub fn quota_exceeded(&self) -> bool {
        self.quota_bytes > 0 && self.total() >= self.quota_bytes
    }

    /// Reset counters (new period).
    pub fn reset(&self) {
        self.bytes_up.store(0, Ordering::Relaxed);
        self.bytes_down.store(0, Ordering::Relaxed);
    }

    /// Remaining bytes in quota (0 if unlimited).
    pub fn remaining(&self) -> u64 {
        if self.quota_bytes == 0 {
            return u64::MAX;
        }
        self.quota_bytes.saturating_sub(self.total())
    }
}

/// Store for all client quotas.
pub struct QuotaStore {
    clients: RwLock<HashMap<String, Arc<ClientUsage>>>,
}

impl Default for QuotaStore {
    fn default() -> Self {
        Self::new()
    }
}

impl QuotaStore {
    pub fn new() -> Self {
        Self {
            clients: RwLock::new(HashMap::new()),
        }
    }

    /// Register a client with a quota.
    pub async fn set_quota(&self, client_id: &str, quota_bytes: u64) {
        let mut clients = self.clients.write().await;
        clients
            .entry(client_id.to_string())
            .or_insert_with(|| Arc::new(ClientUsage::new(quota_bytes)));
    }

    /// Check if a client has any quota configured.
    pub async fn has_client(&self, client_id: &str) -> bool {
        self.clients.read().await.contains_key(client_id)
    }

    /// Get the usage tracker for a client.
    pub async fn get(&self, client_id: &str) -> Option<Arc<ClientUsage>> {
        self.clients.read().await.get(client_id).cloned()
    }

    /// Check if a client has exceeded their quota.
    pub async fn is_quota_exceeded(&self, client_id: &str) -> bool {
        if let Some(usage) = self.get(client_id).await {
            usage.quota_exceeded()
        } else {
            false // no quota configured
        }
    }

    /// Reset all client counters (call at period boundary).
    pub async fn reset_all(&self) {
        let clients = self.clients.read().await;
        for usage in clients.values() {
            usage.reset();
        }
    }

    /// Get a snapshot of all client usage for reporting.
    pub async fn snapshot(&self) -> Vec<(String, u64, u64, u64)> {
        let clients = self.clients.read().await;
        clients
            .iter()
            .map(|(id, u)| {
                (
                    id.clone(),
                    u.bytes_up.load(Ordering::Relaxed),
                    u.bytes_down.load(Ordering::Relaxed),
                    u.quota_bytes,
                )
            })
            .collect()
    }
}

/// Parse a quota string like "100GB", "1TB", "500MB" into bytes.
pub fn parse_quota(s: &str) -> Option<u64> {
    let s = s.trim().to_uppercase();
    if let Some(n) = s.strip_suffix("TB") {
        n.trim().parse::<u64>().ok().map(|n| n * 1_000_000_000_000)
    } else if let Some(n) = s.strip_suffix("GB") {
        n.trim().parse::<u64>().ok().map(|n| n * 1_000_000_000)
    } else if let Some(n) = s.strip_suffix("MB") {
        n.trim().parse::<u64>().ok().map(|n| n * 1_000_000)
    } else if let Some(n) = s.strip_suffix("KB") {
        n.trim().parse::<u64>().ok().map(|n| n * 1_000)
    } else {
        s.parse::<u64>().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_quota() {
        assert_eq!(parse_quota("100GB"), Some(100_000_000_000));
        assert_eq!(parse_quota("1TB"), Some(1_000_000_000_000));
        assert_eq!(parse_quota("500MB"), Some(500_000_000));
        assert_eq!(parse_quota("1024KB"), Some(1_024_000));
        assert_eq!(parse_quota("1024"), Some(1024));
        assert_eq!(parse_quota("invalid"), None);
    }

    #[test]
    fn test_client_usage() {
        let usage = ClientUsage::new(1_000_000); // 1MB quota

        assert!(!usage.quota_exceeded());
        assert_eq!(usage.remaining(), 1_000_000);

        usage.add_upload(500_000);
        usage.add_download(400_000);
        assert!(!usage.quota_exceeded());
        assert_eq!(usage.remaining(), 100_000);

        usage.add_upload(100_000);
        assert!(usage.quota_exceeded());
        assert_eq!(usage.remaining(), 0);
    }

    #[test]
    fn test_unlimited_quota() {
        let usage = ClientUsage::new(0); // unlimited
        usage.add_upload(u64::MAX / 2);
        assert!(!usage.quota_exceeded());
        assert_eq!(usage.remaining(), u64::MAX);
    }

    #[test]
    fn test_reset() {
        let usage = ClientUsage::new(1000);
        usage.add_upload(999);
        assert!(!usage.quota_exceeded());

        usage.add_upload(1);
        assert!(usage.quota_exceeded());

        usage.reset();
        assert!(!usage.quota_exceeded());
        assert_eq!(usage.total(), 0);
    }

    #[tokio::test]
    async fn test_quota_store() {
        let store = QuotaStore::new();

        store.set_quota("client1", 1_000_000).await;
        assert!(!store.is_quota_exceeded("client1").await);

        if let Some(usage) = store.get("client1").await {
            usage.add_upload(1_000_000);
        }
        assert!(store.is_quota_exceeded("client1").await);

        // Unknown client is never exceeded
        assert!(!store.is_quota_exceeded("unknown").await);
    }
}
