pub mod connection_pool;
pub mod connector;
pub mod dns_resolver;
pub mod dns_server;
pub mod forward;
pub mod grpc_stream;
pub mod http;
pub mod proxy;
pub mod relay;
pub mod socks5;
pub mod tun;
pub mod tunnel;
pub mod udp_relay;
pub mod ws_stream;
pub mod xhttp_stream;
pub mod xporta_stream;

use std::sync::Arc;

use anyhow::Result;
use prisma_core::config::load_client_config;
use prisma_core::congestion::CongestionMode;
use prisma_core::logging::init_logging;
use prisma_core::router::Router;
use prisma_core::types::{CipherSuite, ClientId};
use prisma_core::util;
use tracing::info;

use dns_resolver::DnsResolver;
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
        info!(
            count = config.port_forwards.len(),
            "Port forwards configured"
        );
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
    let use_ws = config.transport == "ws";
    let use_grpc = config.transport == "grpc";
    let use_xhttp = config.transport == "xhttp";
    let use_xporta = config.transport == "xporta";

    if use_ws {
        info!(ws_url = ?config.ws_url, "WebSocket transport enabled");
    }
    if use_grpc {
        info!(grpc_url = ?config.grpc_url, "gRPC transport enabled");
    }
    if use_xhttp {
        info!(xhttp_mode = ?config.xhttp_mode, "XHTTP transport enabled");
    }
    if use_xporta {
        info!("XPorta transport enabled");
    }

    let congestion_mode = CongestionMode::from_config(
        &config.congestion.mode,
        config.congestion.target_bandwidth.as_deref(),
    );

    if use_quic {
        info!(mode = ?congestion_mode, "Congestion control configured");
    }

    let ctx = ProxyContext {
        server_addr: config.server_addr.clone(),
        client_id,
        auth_secret,
        cipher_suite,
        use_quic,
        skip_cert_verify: config.skip_cert_verify,
        tls_on_tcp: config.tls_on_tcp,
        alpn_protocols: config.alpn_protocols.clone(),
        tls_server_name: config.tls_server_name.clone(),
        use_ws,
        ws_url: config.ws_url.clone(),
        ws_extra_headers: config.ws_extra_headers.clone(),
        use_grpc,
        grpc_url: config.grpc_url.clone(),
        use_xhttp,
        xhttp_mode: config.xhttp_mode.clone(),
        xhttp_stream_url: config.xhttp_stream_url.clone(),
        xhttp_upload_url: config.xhttp_upload_url.clone(),
        xhttp_download_url: config.xhttp_download_url.clone(),
        xhttp_extra_headers: config.xhttp_extra_headers.clone(),
        use_xporta,
        xporta_config: config.xporta.clone(),
        user_agent: config.user_agent.clone(),
        referer: config.referer.clone(),
        congestion_mode,
        port_hopping: config.port_hopping.clone(),
        salamander_password: config.salamander_password.clone(),
        udp_fec: if config.udp_fec.enabled {
            Some(config.udp_fec.clone())
        } else {
            None
        },
        dns_config: config.dns.clone(),
        dns_resolver: DnsResolver::new(&config.dns),
        router: Arc::new(Router::new(config.routing.rules.clone())),
    };

    // Log DNS mode
    if config.dns.mode != prisma_core::dns::DnsMode::Direct {
        info!(mode = ?config.dns.mode, "DNS mode configured");
    }

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

    // Optionally start local DNS server (for Fake, Tunnel, or Smart modes)
    let dns_handle = if config.dns.mode != prisma_core::dns::DnsMode::Direct {
        let dns_ctx = ctx.clone();
        Some(tokio::spawn(async move {
            if let Err(e) = dns_server::run_dns_server(dns_ctx).await {
                tracing::error!("DNS server error: {}", e);
            }
        }))
    } else {
        None
    };

    // Optionally start TUN mode
    let tun_handle = if config.tun.enabled {
        info!(device = %config.tun.device_name, mtu = config.tun.mtu, "Starting TUN mode");
        match tun::device::create_tun_device(
            &config.tun.device_name,
            config.tun.mtu,
            &config.tun.include_routes,
            &config.tun.exclude_routes,
        ) {
            Ok(device) => {
                let tun_ctx = ctx.clone();
                Some(tokio::spawn(async move {
                    if let Err(e) = tun::handler::run_tun_handler(device, tun_ctx).await {
                        tracing::error!("TUN handler error: {}", e);
                    }
                }))
            }
            Err(e) => {
                tracing::error!("Failed to create TUN device: {}. TUN mode disabled.", e);
                None
            }
        }
    } else {
        None
    };

    // All services run forever; wait for any to exit
    tokio::select! {
        _ = socks5_handle => {},
        _ = async { if let Some(h) = http_handle { h.await.ok(); } else { std::future::pending::<()>().await; } } => {},
        _ = async { if let Some(h) = forward_handle { h.await.ok(); } else { std::future::pending::<()>().await; } } => {},
        _ = async { if let Some(h) = dns_handle { h.await.ok(); } else { std::future::pending::<()>().await; } } => {},
        _ = async { if let Some(h) = tun_handle { h.await.ok(); } else { std::future::pending::<()>().await; } } => {},
    }

    Ok(())
}
