use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use prisma_core::state::ServerState;

#[derive(Serialize)]
pub struct ConfigResponse {
    pub listen_addr: String,
    pub quic_listen_addr: String,
    pub tls_enabled: bool,
    pub max_connections: u32,
    pub connection_timeout_secs: u64,
    pub port_forwarding_enabled: bool,
    pub port_forwarding_range: String,
    pub logging_level: String,
    pub logging_format: String,
    pub camouflage_enabled: bool,
    pub camouflage_tls_on_tcp: bool,
    pub camouflage_fallback_addr: Option<String>,
    pub camouflage_alpn: Vec<String>,
}

pub async fn get_config(State(state): State<ServerState>) -> Json<ConfigResponse> {
    let cfg = state.config.read().await;
    Json(ConfigResponse {
        listen_addr: cfg.listen_addr.clone(),
        quic_listen_addr: cfg.quic_listen_addr.clone(),
        tls_enabled: cfg.tls.is_some(),
        max_connections: cfg.performance.max_connections,
        connection_timeout_secs: cfg.performance.connection_timeout_secs,
        port_forwarding_enabled: cfg.port_forwarding.enabled,
        port_forwarding_range: format!(
            "{}-{}",
            cfg.port_forwarding.port_range_start, cfg.port_forwarding.port_range_end
        ),
        logging_level: cfg.logging.level.clone(),
        logging_format: cfg.logging.format.clone(),
        camouflage_enabled: cfg.camouflage.enabled,
        camouflage_tls_on_tcp: cfg.camouflage.tls_on_tcp,
        camouflage_fallback_addr: cfg.camouflage.fallback_addr.clone(),
        camouflage_alpn: cfg.camouflage.alpn_protocols.clone(),
    })
}

#[derive(Deserialize)]
pub struct PatchConfigRequest {
    pub logging_level: Option<String>,
    pub logging_format: Option<String>,
    pub max_connections: Option<u32>,
    pub port_forwarding_enabled: Option<bool>,
}

pub async fn patch_config(
    State(state): State<ServerState>,
    Json(req): Json<PatchConfigRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    if let Some(ref level) = req.logging_level {
        prisma_core::config::validation::validate_logging_level(level)
            .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    }
    if let Some(ref format) = req.logging_format {
        prisma_core::config::validation::validate_logging_format(format)
            .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    }
    let mut cfg = state.config.write().await;
    if let Some(level) = req.logging_level {
        cfg.logging.level = level;
    }
    if let Some(format) = req.logging_format {
        cfg.logging.format = format;
    }
    if let Some(max) = req.max_connections {
        cfg.performance.max_connections = max;
    }
    if let Some(enabled) = req.port_forwarding_enabled {
        cfg.port_forwarding.enabled = enabled;
    }
    Ok(StatusCode::OK)
}

#[derive(Serialize)]
pub struct TlsInfoResponse {
    pub enabled: bool,
    pub cert_path: Option<String>,
    pub key_path: Option<String>,
}

pub async fn get_tls_info(State(state): State<ServerState>) -> Json<TlsInfoResponse> {
    let cfg = state.config.read().await;
    match &cfg.tls {
        Some(tls) => Json(TlsInfoResponse {
            enabled: true,
            cert_path: Some(tls.cert_path.clone()),
            key_path: Some(tls.key_path.clone()),
        }),
        None => Json(TlsInfoResponse {
            enabled: false,
            cert_path: None,
            key_path: None,
        }),
    }
}
