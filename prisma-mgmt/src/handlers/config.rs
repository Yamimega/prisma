use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::handlers::backup;
use crate::MgmtState;

// --- Camouflage sub-response ---
#[derive(Serialize)]
pub struct CamouflageInfo {
    pub enabled: bool,
    pub tls_on_tcp: bool,
    pub fallback_addr: Option<String>,
    pub alpn_protocols: Vec<String>,
    pub salamander_password: Option<String>,
    pub h3_cover_site: Option<String>,
    pub h3_static_dir: Option<String>,
}

// --- CDN sub-response ---
#[derive(Serialize)]
pub struct CdnInfo {
    pub enabled: bool,
    pub ws_tunnel_path: String,
    pub grpc_tunnel_path: String,
    pub xhttp_upload_path: String,
    pub xhttp_download_path: String,
    pub xhttp_stream_path: String,
    pub cover_upstream: Option<String>,
    pub xporta_enabled: bool,
}

// --- Traffic shaping sub-response ---
#[derive(Serialize)]
pub struct TrafficShapingInfo {
    pub padding_mode: String,
    pub bucket_sizes: Vec<u16>,
    pub timing_jitter_ms: u32,
    pub chaff_interval_ms: u32,
    pub coalesce_window_ms: u32,
}

// --- Congestion sub-response ---
#[derive(Serialize)]
pub struct CongestionInfo {
    pub mode: String,
    pub target_bandwidth: Option<String>,
}

// --- AntiRtt sub-response ---
#[derive(Serialize)]
pub struct AntiRttInfo {
    pub enabled: bool,
    pub normalization_ms: u32,
}

// --- PrismaTLS sub-response ---
#[derive(Serialize)]
pub struct PrismaTlsInfo {
    pub enabled: bool,
    pub mask_server_count: usize,
    pub auth_rotation_hours: u64,
}

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
    // Camouflage (full)
    pub camouflage: CamouflageInfo,
    // CDN
    pub cdn: CdnInfo,
    // Traffic shaping
    pub traffic_shaping: TrafficShapingInfo,
    // Congestion
    pub congestion: CongestionInfo,
    // DNS upstream
    pub dns_upstream: String,
    // Anti-RTT
    pub anti_rtt: AntiRttInfo,
    // PrismaTLS
    pub prisma_tls: PrismaTlsInfo,
    // Transport-only cipher
    pub allow_transport_only_cipher: bool,
    // Protocol version
    pub protocol_version: String,
}

pub async fn get_config(State(state): State<MgmtState>) -> Json<ConfigResponse> {
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
        camouflage: CamouflageInfo {
            enabled: cfg.camouflage.enabled,
            tls_on_tcp: cfg.camouflage.tls_on_tcp,
            fallback_addr: cfg.camouflage.fallback_addr.clone(),
            alpn_protocols: cfg.camouflage.alpn_protocols.clone(),
            salamander_password: cfg.camouflage.salamander_password.clone(),
            h3_cover_site: cfg.camouflage.h3_cover_site.clone(),
            h3_static_dir: cfg.camouflage.h3_static_dir.clone(),
        },
        cdn: CdnInfo {
            enabled: cfg.cdn.enabled,
            ws_tunnel_path: cfg.cdn.ws_tunnel_path.clone(),
            grpc_tunnel_path: cfg.cdn.grpc_tunnel_path.clone(),
            xhttp_upload_path: cfg.cdn.xhttp_upload_path.clone(),
            xhttp_download_path: cfg.cdn.xhttp_download_path.clone(),
            xhttp_stream_path: cfg.cdn.xhttp_stream_path.clone(),
            cover_upstream: cfg.cdn.cover_upstream.clone(),
            xporta_enabled: cfg.cdn.xporta.as_ref().is_some_and(|x| x.enabled),
        },
        traffic_shaping: TrafficShapingInfo {
            padding_mode: cfg.traffic_shaping.padding_mode.clone(),
            bucket_sizes: cfg.traffic_shaping.bucket_sizes.clone(),
            timing_jitter_ms: cfg.traffic_shaping.timing_jitter_ms,
            chaff_interval_ms: cfg.traffic_shaping.chaff_interval_ms,
            coalesce_window_ms: cfg.traffic_shaping.coalesce_window_ms,
        },
        congestion: CongestionInfo {
            mode: cfg.congestion.mode.clone(),
            target_bandwidth: cfg.congestion.target_bandwidth.clone(),
        },
        dns_upstream: cfg.dns_upstream.clone(),
        anti_rtt: AntiRttInfo {
            enabled: cfg.anti_rtt.enabled,
            normalization_ms: cfg.anti_rtt.normalization_ms,
        },
        prisma_tls: PrismaTlsInfo {
            enabled: cfg.prisma_tls.enabled,
            mask_server_count: cfg.prisma_tls.mask_servers.len(),
            auth_rotation_hours: cfg.prisma_tls.auth_rotation_hours,
        },
        allow_transport_only_cipher: cfg.allow_transport_only_cipher,
        protocol_version: cfg.protocol_version.clone(),
    })
}

#[derive(Deserialize)]
pub struct PatchConfigRequest {
    // Logging
    pub logging_level: Option<String>,
    pub logging_format: Option<String>,
    // Performance
    pub max_connections: Option<u32>,
    pub connection_timeout_secs: Option<u64>,
    // Port forwarding
    pub port_forwarding_enabled: Option<bool>,
    // Camouflage
    pub camouflage_enabled: Option<bool>,
    pub camouflage_tls_on_tcp: Option<bool>,
    pub camouflage_fallback_addr: Option<String>,
    // Traffic shaping
    pub traffic_shaping_padding_mode: Option<String>,
    pub traffic_shaping_timing_jitter_ms: Option<u32>,
    pub traffic_shaping_chaff_interval_ms: Option<u32>,
    pub traffic_shaping_coalesce_window_ms: Option<u32>,
    // Congestion
    pub congestion_mode: Option<String>,
    pub congestion_target_bandwidth: Option<String>,
    // DNS
    pub dns_upstream: Option<String>,
    // Anti-RTT
    pub anti_rtt_enabled: Option<bool>,
    pub anti_rtt_normalization_ms: Option<u32>,
    // Transport-only cipher
    pub allow_transport_only_cipher: Option<bool>,
}

pub async fn patch_config(
    State(state): State<MgmtState>,
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

    // Auto-backup before applying changes
    let _ = backup::auto_backup(&state).await;

    let mut cfg = state.config.write().await;

    // Logging
    if let Some(level) = req.logging_level {
        cfg.logging.level = level;
    }
    if let Some(format) = req.logging_format {
        cfg.logging.format = format;
    }

    // Performance
    if let Some(max) = req.max_connections {
        cfg.performance.max_connections = max;
    }
    if let Some(timeout) = req.connection_timeout_secs {
        cfg.performance.connection_timeout_secs = timeout;
    }

    // Port forwarding
    if let Some(enabled) = req.port_forwarding_enabled {
        cfg.port_forwarding.enabled = enabled;
    }

    // Camouflage
    if let Some(enabled) = req.camouflage_enabled {
        cfg.camouflage.enabled = enabled;
    }
    if let Some(tls_on_tcp) = req.camouflage_tls_on_tcp {
        cfg.camouflage.tls_on_tcp = tls_on_tcp;
    }
    if let Some(fallback) = req.camouflage_fallback_addr {
        cfg.camouflage.fallback_addr = Some(fallback);
    }

    // Traffic shaping
    if let Some(mode) = req.traffic_shaping_padding_mode {
        cfg.traffic_shaping.padding_mode = mode;
    }
    if let Some(jitter) = req.traffic_shaping_timing_jitter_ms {
        cfg.traffic_shaping.timing_jitter_ms = jitter;
    }
    if let Some(chaff) = req.traffic_shaping_chaff_interval_ms {
        cfg.traffic_shaping.chaff_interval_ms = chaff;
    }
    if let Some(coalesce) = req.traffic_shaping_coalesce_window_ms {
        cfg.traffic_shaping.coalesce_window_ms = coalesce;
    }

    // Congestion
    if let Some(mode) = req.congestion_mode {
        cfg.congestion.mode = mode;
    }
    if let Some(target) = req.congestion_target_bandwidth {
        cfg.congestion.target_bandwidth = Some(target);
    }

    // DNS
    if let Some(upstream) = req.dns_upstream {
        cfg.dns_upstream = upstream;
    }

    // Anti-RTT
    if let Some(enabled) = req.anti_rtt_enabled {
        cfg.anti_rtt.enabled = enabled;
    }
    if let Some(ms) = req.anti_rtt_normalization_ms {
        cfg.anti_rtt.normalization_ms = ms;
    }

    // Transport-only cipher
    if let Some(allow) = req.allow_transport_only_cipher {
        cfg.allow_transport_only_cipher = allow;
    }

    Ok(StatusCode::OK)
}

#[derive(Serialize)]
pub struct TlsInfoResponse {
    pub enabled: bool,
    pub cert_path: Option<String>,
    pub key_path: Option<String>,
}

pub async fn get_tls_info(State(state): State<MgmtState>) -> Json<TlsInfoResponse> {
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
