use std::sync::atomic::Ordering;
use std::sync::Arc;

use anyhow::Result;
use tokio::net::TcpListener;
use tokio::sync::Semaphore;
use tracing::{info, warn};

use prisma_core::cache::DnsCache;
use prisma_core::config::server::ServerConfig;

use prisma_core::state::ServerState;

use crate::auth::AuthStore;
use crate::handler;

pub async fn listen(
    config: &ServerConfig,
    auth: AuthStore,
    dns_cache: DnsCache,
    state: ServerState,
) -> Result<()> {
    let listener = TcpListener::bind(&config.listen_addr).await?;
    let max_conn = config.performance.max_connections as usize;
    let semaphore = Arc::new(Semaphore::new(max_conn));
    info!(addr = %config.listen_addr, max_connections = max_conn, "TCP listener started");

    loop {
        match listener.accept().await {
            Ok((stream, peer_addr)) => {
                let permit = match semaphore.clone().try_acquire_owned() {
                    Ok(p) => p,
                    Err(_) => {
                        warn!(peer = %peer_addr, "Connection rejected: max connections reached");
                        drop(stream);
                        continue;
                    }
                };
                let auth = auth.clone();
                let dns = dns_cache.clone();
                let fwd = config.port_forwarding.clone();
                let state = state.clone();
                tokio::spawn(async move {
                    info!(peer = %peer_addr, "New TCP connection");
                    state
                        .metrics
                        .total_connections
                        .fetch_add(1, Ordering::Relaxed);
                    state
                        .metrics
                        .active_connections
                        .fetch_add(1, Ordering::Relaxed);
                    if let Err(e) = handler::handle_tcp_connection(
                        stream,
                        auth,
                        dns,
                        fwd,
                        state.clone(),
                        peer_addr.to_string(),
                    )
                    .await
                    {
                        warn!(peer = %peer_addr, error = %e, "Connection handler error");
                    }
                    state
                        .metrics
                        .active_connections
                        .fetch_sub(1, Ordering::Relaxed);
                    drop(permit);
                });
            }
            Err(e) => {
                warn!(error = %e, "Failed to accept TCP connection");
            }
        }
    }
}
