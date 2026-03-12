pub mod auth;
pub mod forward;
pub mod handler;
pub mod listener;
pub mod outbound;
pub mod relay;
pub mod state;

use anyhow::Result;
use prisma_core::cache::DnsCache;
use prisma_core::config::load_server_config;
use prisma_core::logging::{init_logging_with_broadcast, BroadcastLogEntry};
use prisma_core::state::{LogEntry, MetricsSnapshot, ServerState};
use tracing::info;

use prisma_core::state::AuthStoreInner;

use crate::auth::AuthStore;

pub async fn run(config_path: &str) -> Result<()> {
    let config = load_server_config(config_path)
        .map_err(|e| anyhow::anyhow!("Failed to load config: {}", e))?;

    // Create broadcast channels
    let (log_tx, _) = tokio::sync::broadcast::channel::<BroadcastLogEntry>(1024);
    let (metrics_tx, _) = tokio::sync::broadcast::channel::<MetricsSnapshot>(256);
    let (state_log_tx, _) = tokio::sync::broadcast::channel::<LogEntry>(1024);

    // Initialize logging with broadcast
    init_logging_with_broadcast(&config.logging.level, &config.logging.format, log_tx);

    info!("Prisma server starting");
    info!(listen = %config.listen_addr, quic_listen = %config.quic_listen_addr);
    info!(
        authorized_clients = config.authorized_clients.len(),
        "Loaded client configurations"
    );

    let auth_inner = AuthStoreInner::from_config(&config.authorized_clients)?;
    let state = ServerState::new(&config, auth_inner, state_log_tx, metrics_tx);
    let auth_store = AuthStore::from_inner(state.auth_store.clone());
    let dns_cache = DnsCache::default();

    // Start metrics ticker (1s snapshots)
    tokio::spawn(prisma_core::state::metrics_ticker(state.clone()));

    // Start management API if enabled
    if config.management_api.enabled {
        let mgmt_state = state.clone();
        let mgmt_config = config.management_api.clone();
        tokio::spawn(async move {
            if let Err(e) = prisma_mgmt::serve(mgmt_config, mgmt_state).await {
                tracing::error!("Management API error: {}", e);
            }
        });
    }

    // Start TCP and QUIC listeners concurrently
    let tcp_config = config.clone();
    let tcp_auth = auth_store.clone();
    let tcp_dns = dns_cache.clone();
    let tcp_state = state.clone();
    let tcp_handle = tokio::spawn(async move {
        if let Err(e) = listener::tcp::listen(&tcp_config, tcp_auth, tcp_dns, tcp_state).await {
            tracing::error!("TCP listener error: {}", e);
        }
    });

    let quic_config = config.clone();
    let quic_auth = auth_store.clone();
    let quic_dns = dns_cache.clone();
    let quic_state = state.clone();
    let quic_handle = tokio::spawn(async move {
        if let Err(e) = listener::quic::listen(&quic_config, quic_auth, quic_dns, quic_state).await
        {
            tracing::error!("QUIC listener error: {}", e);
        }
    });

    tokio::select! {
        _ = tcp_handle => {},
        _ = quic_handle => {},
    }

    Ok(())
}
