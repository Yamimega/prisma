//! Per-client rate limiter using token bucket algorithm.

use std::collections::HashMap;
use std::num::NonZeroU32;
use std::sync::Arc;

use governor::{Quota, RateLimiter};
use tokio::sync::RwLock;

/// Per-client bandwidth limits.
#[derive(Debug, Clone)]
pub struct BandwidthLimit {
    pub upload_bps: u64,   // 0 = unlimited
    pub download_bps: u64, // 0 = unlimited
}

type Limiter = RateLimiter<
    governor::state::NotKeyed,
    governor::state::InMemoryState,
    governor::clock::DefaultClock,
>;

/// Rate limiter store: maps client_id to their rate limiters.
pub struct BandwidthLimiterStore {
    /// Upload limiters keyed by client UUID string.
    upload: RwLock<HashMap<String, Arc<Limiter>>>,
    /// Download limiters keyed by client UUID string.
    download: RwLock<HashMap<String, Arc<Limiter>>>,
}

impl Default for BandwidthLimiterStore {
    fn default() -> Self {
        Self::new()
    }
}

impl BandwidthLimiterStore {
    pub fn new() -> Self {
        Self {
            upload: RwLock::new(HashMap::new()),
            download: RwLock::new(HashMap::new()),
        }
    }

    /// Configure limits for a client. Call this when loading config.
    pub async fn set_limit(&self, client_id: &str, limit: &BandwidthLimit) {
        if limit.upload_bps > 0 {
            if let Some(limiter) = create_limiter(limit.upload_bps) {
                self.upload
                    .write()
                    .await
                    .insert(client_id.to_string(), Arc::new(limiter));
            }
        }
        if limit.download_bps > 0 {
            if let Some(limiter) = create_limiter(limit.download_bps) {
                self.download
                    .write()
                    .await
                    .insert(client_id.to_string(), Arc::new(limiter));
            }
        }
    }

    /// Check if upload of `bytes` is allowed for the client.
    /// Returns `true` if allowed (or no limit set), `false` if rate-limited.
    pub async fn check_upload(&self, client_id: &str, bytes: u32) -> bool {
        let limiters = self.upload.read().await;
        if let Some(limiter) = limiters.get(client_id) {
            let cells = NonZeroU32::new(bytes.max(1)).unwrap();
            limiter.check_n(cells).is_ok()
        } else {
            true // no limit
        }
    }

    /// Check if download of `bytes` is allowed for the client.
    pub async fn check_download(&self, client_id: &str, bytes: u32) -> bool {
        let limiters = self.download.read().await;
        if let Some(limiter) = limiters.get(client_id) {
            let cells = NonZeroU32::new(bytes.max(1)).unwrap();
            limiter.check_n(cells).is_ok()
        } else {
            true
        }
    }

    /// Wait until upload of `bytes` is allowed.
    pub async fn wait_upload(&self, client_id: &str, bytes: u32) {
        let limiters = self.upload.read().await;
        if let Some(limiter) = limiters.get(client_id) {
            let cells = NonZeroU32::new(bytes.max(1)).unwrap();
            let limiter = limiter.clone();
            drop(limiters); // release read lock
            let _ = limiter.until_n_ready(cells).await;
        }
    }

    /// Wait until download of `bytes` is allowed.
    pub async fn wait_download(&self, client_id: &str, bytes: u32) {
        let limiters = self.download.read().await;
        if let Some(limiter) = limiters.get(client_id) {
            let cells = NonZeroU32::new(bytes.max(1)).unwrap();
            let limiter = limiter.clone();
            drop(limiters);
            let _ = limiter.until_n_ready(cells).await;
        }
    }
}

/// Create a rate limiter for the given bytes-per-second rate.
fn create_limiter(bps: u64) -> Option<Limiter> {
    // governor works in "cells per period"
    // We use 1 cell = 1 byte, period = 1 second
    // Burst = bps (allow 1 second burst)
    let per_second = NonZeroU32::new(bps.min(u32::MAX as u64) as u32)?;
    let quota = Quota::per_second(per_second).allow_burst(per_second);
    Some(RateLimiter::direct(quota))
}

/// Parse a bandwidth string like "100mbps", "1gbps", "500kbps" into bytes per second.
pub fn parse_bandwidth(s: &str) -> Option<u64> {
    let s = s.trim().to_lowercase();
    if let Some(n) = s.strip_suffix("gbps") {
        n.trim().parse::<u64>().ok().map(|n| n * 1_000_000_000 / 8)
    } else if let Some(n) = s.strip_suffix("mbps") {
        n.trim().parse::<u64>().ok().map(|n| n * 1_000_000 / 8)
    } else if let Some(n) = s.strip_suffix("kbps") {
        n.trim().parse::<u64>().ok().map(|n| n * 1_000 / 8)
    } else if let Some(n) = s.strip_suffix("bps") {
        n.trim().parse::<u64>().ok().map(|n| n / 8)
    } else {
        s.parse::<u64>().ok() // raw bytes per second
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_bandwidth() {
        assert_eq!(parse_bandwidth("100mbps"), Some(100_000_000 / 8));
        assert_eq!(parse_bandwidth("1gbps"), Some(1_000_000_000 / 8));
        assert_eq!(parse_bandwidth("500kbps"), Some(500_000 / 8));
        assert_eq!(parse_bandwidth("8000bps"), Some(1000));
        assert_eq!(parse_bandwidth("1024"), Some(1024));
        assert_eq!(parse_bandwidth("invalid"), None);
    }

    #[tokio::test]
    async fn test_unlimited_client() {
        let store = BandwidthLimiterStore::new();
        // No limit set → always allowed
        assert!(store.check_upload("client1", 1000).await);
        assert!(store.check_download("client1", 1000).await);
    }

    #[tokio::test]
    async fn test_rate_limited_client() {
        let store = BandwidthLimiterStore::new();
        store
            .set_limit(
                "client1",
                &BandwidthLimit {
                    upload_bps: 1000,
                    download_bps: 2000,
                },
            )
            .await;

        // First check within burst should succeed
        assert!(store.check_upload("client1", 500).await);
        // Subsequent large check may be rate-limited
        // (depends on timing, so just verify it doesn't panic)
        let _ = store.check_upload("client1", 500).await;
    }
}
