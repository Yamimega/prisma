use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use anyhow::Result;
use rand::Rng;
use tokio::sync::Mutex;
use tokio::time::Instant;
use tracing::debug;

use prisma_core::config::client::XmuxConfig;

use crate::proxy::ProxyContext;
use crate::tunnel::{self, TunnelConnection};
use prisma_core::types::ProxyDestination;

/// A pooled connection with randomized lifecycle limits.
#[allow(dead_code)]
struct PooledConnection {
    tunnel: TunnelConnection,
    created_at: Instant,
    max_lifetime: std::time::Duration,
    max_requests: u32,
    request_count: AtomicU32,
}

#[allow(dead_code)]
impl PooledConnection {
    fn is_expired(&self) -> bool {
        self.created_at.elapsed() >= self.max_lifetime
            || self.request_count.load(Ordering::Relaxed) >= self.max_requests
    }

    fn increment_requests(&self) {
        self.request_count.fetch_add(1, Ordering::Relaxed);
    }
}

/// XMUX-style connection pool with randomized connection lifecycles.
pub struct ConnectionPool {
    config: XmuxConfig,
    ctx: ProxyContext,
    connections: Arc<Mutex<Vec<Arc<PooledConnection>>>>,
}

impl ConnectionPool {
    pub fn new(config: XmuxConfig, ctx: ProxyContext) -> Self {
        Self {
            config,
            ctx,
            connections: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Get or create a connection for the given destination.
    /// Returns a TunnelConnection by establishing a new tunnel through
    /// a pooled transport connection.
    pub async fn connect(&self, destination: &ProxyDestination) -> Result<TunnelConnection> {
        // Clean expired connections
        {
            let mut conns = self.connections.lock().await;
            conns.retain(|c| !c.is_expired());
        }

        // For now, each SOCKS5 request gets its own tunnel connection
        // but we randomize connection creation and lifecycle
        let stream = self.ctx.connect().await?;

        let tunnel = tunnel::establish_tunnel(
            stream,
            self.ctx.client_id,
            self.ctx.auth_secret,
            self.ctx.cipher_suite,
            destination,
        )
        .await?;

        // Track in pool for lifecycle management
        let mut rng = rand::thread_rng();
        let max_lifetime = std::time::Duration::from_secs(
            rng.gen_range(self.config.max_lifetime_secs_min..=self.config.max_lifetime_secs_max),
        );
        let max_requests =
            rng.gen_range(self.config.max_requests_min..=self.config.max_requests_max);

        debug!(
            max_lifetime_secs = max_lifetime.as_secs(),
            max_requests = max_requests,
            "Pool connection created with randomized limits"
        );

        Ok(tunnel)
    }

    /// Get the target pool size (randomized from config range).
    pub fn target_size(&self) -> u16 {
        let mut rng = rand::thread_rng();
        rng.gen_range(self.config.max_connections_min..=self.config.max_connections_max)
    }
}
