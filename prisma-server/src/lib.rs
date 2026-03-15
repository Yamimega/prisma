pub mod auth;
pub mod bandwidth;
pub mod camouflage;
pub mod channel_stream;
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
use prisma_core::config::server::RoutingRule;
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

    // Print startup banner before logging init — always visible regardless of log level
    print_startup_banner(&config, config_path);

    // Create broadcast channels
    let (log_tx, _) = tokio::sync::broadcast::channel::<LogEntry>(1024);
    let (metrics_tx, _) = tokio::sync::broadcast::channel::<MetricsSnapshot>(256);

    // Initialize logging with broadcast
    init_logging_with_broadcast(
        &config.logging.level,
        &config.logging.format,
        log_tx.clone(),
    );

    let auth_inner = AuthStoreInner::from_config(&config.authorized_clients)?;
    let state = ServerState::new(&config, auth_inner, log_tx, metrics_tx);

    // Load static routing rules from config
    if !config.routing.rules.is_empty() {
        let static_rules: Vec<RoutingRule> = config
            .routing
            .rules
            .iter()
            .enumerate()
            .map(|(i, rule)| RoutingRule::from_router_rule(rule, 10000 + i as u32))
            .collect();
        let count = static_rules.len();
        state.routing_rules.write().await.extend(static_rules);
        info!(count, "Loaded static routing rules from config");
    }

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
        let mut mgmt_config = config.management_api.clone();

        // Inherit TLS from server config if not explicitly set on management API
        if mgmt_config.tls.is_none() && mgmt_config.tls_enabled {
            mgmt_config.tls = config.tls.clone();
        }
        if !mgmt_config.tls_enabled {
            mgmt_config.tls = None;
        }

        // Load alert config from {config_dir}/alerts.json if it exists
        let config_path_buf = std::path::PathBuf::from(config_path);
        let alert_config = {
            let alerts_path = config_path_buf
                .parent()
                .unwrap_or(std::path::Path::new("."))
                .join("alerts.json");
            if alerts_path.exists() {
                std::fs::read_to_string(&alerts_path)
                    .ok()
                    .and_then(|s| serde_json::from_str::<prisma_mgmt::AlertConfig>(&s).ok())
                    .unwrap_or_default()
            } else {
                prisma_mgmt::AlertConfig::default()
            }
        };

        let mgmt_state = prisma_mgmt::MgmtState {
            state: state.clone(),
            bandwidth: Some(ctx.bandwidth.clone()),
            quotas: Some(ctx.quotas.clone()),
            config_path: Some(config_path_buf),
            alert_config: std::sync::Arc::new(tokio::sync::RwLock::new(alert_config)),
        };

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

fn print_startup_banner(config: &prisma_core::config::server::ServerConfig, config_path: &str) {
    use prisma_core::types::PRISMA_PROTOCOL_VERSION;

    eprintln!();
    eprintln!(
        "  Prisma v{} (protocol v{})",
        env!("CARGO_PKG_VERSION"),
        PRISMA_PROTOCOL_VERSION
    );
    eprintln!("  ─────────────────────────────────────");
    eprintln!("  TCP    listening on  {}", config.listen_addr);
    eprintln!("  QUIC   listening on  {}", config.quic_listen_addr);
    if config.management_api.enabled {
        let mgmt_tls = if config.management_api.tls_enabled
            && (config.management_api.tls.is_some() || config.tls.is_some())
        {
            "HTTPS"
        } else {
            "HTTP"
        };
        eprintln!(
            "  API    listening on  {} ({})",
            config.management_api.listen_addr, mgmt_tls
        );
    }
    if config.cdn.enabled {
        eprintln!("  CDN    listening on  {}", config.cdn.listen_addr);
    }
    eprintln!("  ─────────────────────────────────────");
    eprintln!("  Config:    {}", config_path);
    eprintln!("  Clients:   {}", config.authorized_clients.len());
    eprintln!("  Log level: {}", config.logging.level);

    // Transport features
    let mut transports = vec!["TCP", "QUIC"];
    if config.cdn.enabled {
        transports.push("WS");
        transports.push("gRPC");
        transports.push("XHTTP");
        if config.cdn.xporta.as_ref().is_some_and(|x| x.enabled) {
            transports.push("XPorta");
        }
    }
    if config.prisma_tls.enabled {
        transports.push("PrismaTLS");
    }
    eprintln!("  Transports: {}", transports.join(", "));

    // Security features
    let tls_status = if config.tls.is_some() {
        "enabled"
    } else {
        "disabled"
    };
    let camo_status = if config.camouflage.enabled {
        "enabled"
    } else {
        "disabled"
    };
    eprintln!("  TLS: {}  Camouflage: {}", tls_status, camo_status);
    eprintln!();
}
