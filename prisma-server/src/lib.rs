pub mod auth;
pub mod bandwidth;
pub mod camouflage;
pub mod forward;
pub mod grpc_stream;
pub mod handler;
pub mod listener;
pub mod outbound;
pub mod relay;
pub mod state;
pub mod udp_relay;
pub mod ws_stream;
pub mod xhttp_stream;
pub mod xporta_stream;

use std::sync::Arc;

use anyhow::Result;
use prisma_core::cache::DnsCache;
use prisma_core::config::load_server_config;
use prisma_core::logging::init_logging_with_broadcast;
use prisma_core::state::{LogEntry, MetricsSnapshot, ServerState};
use tracing::info;

use prisma_core::state::AuthStoreInner;

use crate::auth::AuthStore;
use crate::bandwidth::limiter::{parse_bandwidth, BandwidthLimit, BandwidthLimiterStore};
use crate::bandwidth::quota::{parse_quota, QuotaStore};
use crate::state::ServerContext;

pub async fn run(config_path: &str) -> Result<()> {
    let config = load_server_config(config_path)
        .map_err(|e| anyhow::anyhow!("Failed to load config: {}", e))?;

    // Create broadcast channels
    let (log_tx, _) = tokio::sync::broadcast::channel::<LogEntry>(1024);
    let (metrics_tx, _) = tokio::sync::broadcast::channel::<MetricsSnapshot>(256);

    // Initialize logging with broadcast
    init_logging_with_broadcast(
        &config.logging.level,
        &config.logging.format,
        log_tx.clone(),
    );

    info!("Prisma server starting");
    info!(listen = %config.listen_addr, quic_listen = %config.quic_listen_addr);
    info!(
        authorized_clients = config.authorized_clients.len(),
        "Loaded client configurations"
    );

    let auth_inner = AuthStoreInner::from_config(&config.authorized_clients)?;
    let state = ServerState::new(&config, auth_inner, log_tx, metrics_tx);
    let auth_store = AuthStore::from_inner(state.auth_store.clone());
    let dns_cache = DnsCache::default();

    // Initialize bandwidth limiter and quota stores from client config
    let bandwidth = Arc::new(BandwidthLimiterStore::new());
    let quotas = Arc::new(QuotaStore::new());

    for client in &config.authorized_clients {
        let upload_bps = client
            .bandwidth_up
            .as_deref()
            .and_then(parse_bandwidth)
            .unwrap_or(0);
        let download_bps = client
            .bandwidth_down
            .as_deref()
            .and_then(parse_bandwidth)
            .unwrap_or(0);
        if upload_bps > 0 || download_bps > 0 {
            bandwidth
                .set_limit(
                    &client.id,
                    &BandwidthLimit {
                        upload_bps,
                        download_bps,
                    },
                )
                .await;
            info!(
                client_id = %client.id,
                up = upload_bps,
                down = download_bps,
                "Configured bandwidth limits"
            );
        }
        if let Some(quota_str) = &client.quota {
            if let Some(quota_bytes) = parse_quota(quota_str) {
                quotas.set_quota(&client.id, quota_bytes).await;
                info!(
                    client_id = %client.id,
                    quota_bytes,
                    "Configured traffic quota"
                );
            }
        }
    }

    let ctx = ServerContext {
        state: state.clone(),
        bandwidth,
        quotas,
    };

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

    // Start CDN listener if enabled
    if config.cdn.enabled {
        let cdn_config = config.clone();
        let cdn_auth = auth_store.clone();
        let cdn_dns = dns_cache.clone();
        let cdn_ctx = ctx.clone();
        tokio::spawn(async move {
            if let Err(e) = listener::cdn::listen(&cdn_config, cdn_auth, cdn_dns, cdn_ctx).await {
                tracing::error!("CDN listener error: {}", e);
            }
        });
        info!(addr = %config.cdn.listen_addr, "CDN HTTPS listener spawned");
    }

    // Start TCP and QUIC listeners concurrently
    let tcp_config = config.clone();
    let tcp_auth = auth_store.clone();
    let tcp_dns = dns_cache.clone();
    let tcp_ctx = ctx.clone();
    let tcp_handle = tokio::spawn(async move {
        if let Err(e) = listener::tcp::listen(&tcp_config, tcp_auth, tcp_dns, tcp_ctx).await {
            tracing::error!("TCP listener error: {}", e);
        }
    });

    let quic_config = config.clone();
    let quic_auth = auth_store.clone();
    let quic_dns = dns_cache.clone();
    let quic_ctx = ctx.clone();
    let quic_handle = tokio::spawn(async move {
        if let Err(e) = listener::quic::listen(&quic_config, quic_auth, quic_dns, quic_ctx).await {
            tracing::error!("QUIC listener error: {}", e);
        }
    });

    tokio::select! {
        _ = tcp_handle => {},
        _ = quic_handle => {},
    }

    Ok(())
}
