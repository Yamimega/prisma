use axum::extract::State;
use axum::Json;
use serde::Serialize;

use crate::MgmtState;

#[derive(Serialize)]
pub struct SystemInfoResponse {
    pub version: String,
    pub platform: String,
    pub pid: u32,
    pub cpu_usage: f32,
    pub memory_used_mb: u64,
    pub memory_total_mb: u64,
    pub listeners: Vec<ListenerInfo>,
    pub cert_expiry_days: Option<i64>,
}

#[derive(Serialize)]
pub struct ListenerInfo {
    pub addr: String,
    pub protocol: String,
}

pub async fn get_system_info(State(state): State<MgmtState>) -> Json<SystemInfoResponse> {
    use sysinfo::System;
    let mut sys = System::new();
    sys.refresh_cpu_all();
    sys.refresh_memory();

    let config = state.config.read().await;
    let mut listeners = vec![
        ListenerInfo {
            addr: config.listen_addr.clone(),
            protocol: "TCP".into(),
        },
        ListenerInfo {
            addr: config.quic_listen_addr.clone(),
            protocol: "QUIC".into(),
        },
    ];
    if config.management_api.enabled {
        listeners.push(ListenerInfo {
            addr: config.management_api.listen_addr.clone(),
            protocol: "Management API".into(),
        });
    }

    // Try to get cert expiry if TLS is configured
    let cert_expiry_days = if let Some(tls) = &config.tls {
        get_cert_expiry_days(&tls.cert_path)
    } else {
        None
    };

    Json(SystemInfoResponse {
        version: env!("CARGO_PKG_VERSION").to_string(),
        platform: std::env::consts::OS.to_string(),
        pid: std::process::id(),
        cpu_usage: sys.global_cpu_usage(),
        memory_used_mb: sys.used_memory() / 1024 / 1024,
        memory_total_mb: sys.total_memory() / 1024 / 1024,
        listeners,
        cert_expiry_days,
    })
}

fn get_cert_expiry_days(cert_path: &str) -> Option<i64> {
    let pem = std::fs::read(cert_path).ok()?;
    let (_, pem_parsed) = x509_parser::pem::parse_x509_pem(&pem).ok()?;
    let (_, cert) = x509_parser::parse_x509_certificate(pem_parsed.contents.as_ref()).ok()?;
    let expiry_epoch = cert.validity().not_after.timestamp();
    let now_epoch = chrono::Utc::now().timestamp();
    Some((expiry_epoch - now_epoch) / 86400)
}
