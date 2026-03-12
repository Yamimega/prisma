pub mod connector;
pub mod forward;
pub mod http;
pub mod proxy;
pub mod relay;
pub mod socks5;
pub mod tunnel;

use anyhow::Result;
use prisma_core::config::load_client_config;
use prisma_core::logging::init_logging;
use prisma_core::types::{CipherSuite, ClientId};
use prisma_core::util;
use tracing::info;

use proxy::ProxyContext;

pub async fn run(config_path: &str) -> Result<()> {
    let config = load_client_config(config_path)
        .map_err(|e| anyhow::anyhow!("Failed to load config: {}", e))?;

    init_logging(&config.logging.level, &config.logging.format);

    info!("Prisma client starting");
    info!(socks5 = %config.socks5_listen_addr, server = %config.server_addr);
    if let Some(ref http_addr) = config.http_listen_addr {
        info!(http = %http_addr, "HTTP proxy enabled");
    }
    if !config.port_forwards.is_empty() {
        info!(count = config.port_forwards.len(), "Port forwards configured");
    }

    let client_id = ClientId::from_uuid(
        uuid::Uuid::parse_str(&config.identity.client_id)
            .map_err(|e| anyhow::anyhow!("Invalid client_id: {}", e))?,
    );

    let auth_secret = util::hex_decode_32(&config.identity.auth_secret)
        .map_err(|e| anyhow::anyhow!("Invalid auth_secret: {}", e))?;

    let cipher_suite = match config.cipher_suite.as_str() {
        "aes-256-gcm" => CipherSuite::Aes256Gcm,
        _ => CipherSuite::ChaCha20Poly1305,
    };

    let use_quic = config.transport == "quic";

    let ctx = ProxyContext {
        server_addr: config.server_addr.clone(),
        client_id,
        auth_secret,
        cipher_suite,
        use_quic,
        skip_cert_verify: config.skip_cert_verify,
    };

    // Start SOCKS5 server
    let socks5_addr = config.socks5_listen_addr.clone();
    let socks5_ctx = ctx.clone();
    let socks5_handle = tokio::spawn(async move {
        if let Err(e) = socks5::server::run_socks5_server(&socks5_addr, socks5_ctx).await {
            tracing::error!("SOCKS5 server error: {}", e);
        }
    });

    // Optionally start HTTP proxy server
    let http_handle = if let Some(ref http_addr) = config.http_listen_addr {
        let http_addr = http_addr.clone();
        let http_ctx = ctx.clone();
        Some(tokio::spawn(async move {
            if let Err(e) = http::server::run_http_proxy(&http_addr, http_ctx).await {
                tracing::error!("HTTP proxy server error: {}", e);
            }
        }))
    } else {
        None
    };

    // Optionally start port forwarding
    let forward_handle = if !config.port_forwards.is_empty() {
        let fwd_ctx = ctx.clone();
        let forwards = config.port_forwards.clone();
        Some(tokio::spawn(async move {
            if let Err(e) = forward::run_port_forwards(fwd_ctx, forwards).await {
                tracing::error!("Port forwarding error: {}", e);
            }
        }))
    } else {
        None
    };

    // All services run forever; wait for any to exit
    tokio::select! {
        _ = socks5_handle => {},
        _ = async { if let Some(h) = http_handle { h.await.ok(); } else { std::future::pending::<()>().await; } } => {},
        _ = async { if let Some(h) = forward_handle { h.await.ok(); } else { std::future::pending::<()>().await; } } => {},
    }

    Ok(())
}
