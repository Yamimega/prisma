use std::net::SocketAddr;
use std::sync::atomic::Ordering;

use axum::extract::ws::WebSocketUpgrade;
use axum::extract::{ConnectInfo, State};
use axum::http::HeaderMap;
use axum::response::Response;
use tracing::{info, warn};

use prisma_core::cache::DnsCache;
use prisma_core::config::server::ServerConfig;

use crate::auth::AuthStore;
use crate::handler;
use crate::state::ServerContext;
use crate::ws_stream::WsStream;

#[derive(Clone)]
pub struct CdnState {
    pub config: ServerConfig,
    pub auth: AuthStore,
    pub dns: DnsCache,
    pub ctx: ServerContext,
    pub trusted_proxies: Vec<String>,
}

pub async fn ws_tunnel_handler(
    ws: WebSocketUpgrade,
    State(cdn): State<CdnState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Response {
    let peer_ip =
        extract_real_ip(&headers, &cdn.trusted_proxies).unwrap_or_else(|| addr.to_string());

    ws.on_upgrade(move |socket| async move {
        info!(peer = %peer_ip, "WebSocket tunnel connection");

        cdn.ctx
            .state
            .metrics
            .total_connections
            .fetch_add(1, Ordering::Relaxed);
        cdn.ctx
            .state
            .metrics
            .active_connections
            .fetch_add(1, Ordering::Relaxed);

        let ws_stream = WsStream::new(socket);
        let fwd = cdn.config.port_forwarding.clone();

        let result = handler::handle_tcp_connection_camouflaged(
            ws_stream,
            cdn.auth,
            cdn.dns,
            fwd,
            cdn.ctx.clone(),
            peer_ip.clone(),
            None, // No fallback for WebSocket — already past HTTP upgrade
        )
        .await;

        if let Err(e) = result {
            warn!(peer = %peer_ip, error = %e, "WebSocket tunnel error");
        }

        cdn.ctx
            .state
            .metrics
            .active_connections
            .fetch_sub(1, Ordering::Relaxed);
    })
}

/// Extract the real client IP from common proxy headers.
fn extract_real_ip(headers: &HeaderMap, _trusted_proxies: &[String]) -> Option<String> {
    // CF-Connecting-IP is set by Cloudflare
    if let Some(val) = headers.get("cf-connecting-ip") {
        if let Ok(ip) = val.to_str() {
            return Some(ip.to_string());
        }
    }
    // X-Real-IP
    if let Some(val) = headers.get("x-real-ip") {
        if let Ok(ip) = val.to_str() {
            return Some(ip.to_string());
        }
    }
    // X-Forwarded-For (first IP)
    if let Some(val) = headers.get("x-forwarded-for") {
        if let Ok(s) = val.to_str() {
            if let Some(first) = s.split(',').next() {
                return Some(first.trim().to_string());
            }
        }
    }
    None
}
