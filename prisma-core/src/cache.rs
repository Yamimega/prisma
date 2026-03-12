use moka::future::Cache;
use std::net::IpAddr;
use std::time::Duration;

/// DNS resolution cache backed by Moka.
#[derive(Clone)]
pub struct DnsCache {
    inner: Cache<String, Vec<IpAddr>>,
}

impl DnsCache {
    pub fn new(max_capacity: u64, ttl_secs: u64) -> Self {
        let cache = Cache::builder()
            .max_capacity(max_capacity)
            .time_to_live(Duration::from_secs(ttl_secs))
            .build();
        Self { inner: cache }
    }

    pub async fn get(&self, hostname: &str) -> Option<Vec<IpAddr>> {
        self.inner.get(hostname).await
    }

    pub async fn insert(&self, hostname: String, addrs: Vec<IpAddr>) {
        self.inner.insert(hostname, addrs).await;
    }

    /// Resolve a hostname, using the cache if available.
    /// Falls back to tokio's DNS resolution.
    pub async fn resolve(&self, hostname: &str) -> std::io::Result<Vec<IpAddr>> {
        if let Some(cached) = self.get(hostname).await {
            return Ok(cached);
        }

        let addrs: Vec<IpAddr> = tokio::net::lookup_host(format!("{}:0", hostname))
            .await?
            .map(|sa| sa.ip())
            .collect();

        if !addrs.is_empty() {
            self.insert(hostname.to_string(), addrs.clone()).await;
        }

        Ok(addrs)
    }
}

impl Default for DnsCache {
    fn default() -> Self {
        Self::new(10_000, 300) // 10k entries, 5 min TTL
    }
}
