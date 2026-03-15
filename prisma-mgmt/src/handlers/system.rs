use axum::extract::State;
use axum::Json;
use serde::Serialize;

use crate::MgmtState;

const SECS_PER_DAY: i64 = 86_400;

#[derive(Serialize)]
pub struct SystemInfoResponse {
    pub version: &'static str,
    pub platform: &'static str,
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

struct SystemMetrics {
    cpu_usage: f32,
    memory_used_mb: u64,
    memory_total_mb: u64,
}

pub async fn get_system_info(State(state): State<MgmtState>) -> Json<SystemInfoResponse> {
    let metrics = get_system_metrics();

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

    let cert_expiry_days = config
        .tls
        .as_ref()
        .and_then(|tls| get_cert_expiry_days(&tls.cert_path));

    Json(SystemInfoResponse {
        version: env!("CARGO_PKG_VERSION"),
        platform: std::env::consts::OS,
        pid: std::process::id(),
        cpu_usage: metrics.cpu_usage,
        memory_used_mb: metrics.memory_used_mb,
        memory_total_mb: metrics.memory_total_mb,
        listeners,
        cert_expiry_days,
    })
}

#[cfg(feature = "sysinfo")]
fn get_system_metrics() -> SystemMetrics {
    use sysinfo::System;

    const BYTES_PER_MB: u64 = 1024 * 1024;

    let mut sys = System::new();
    sys.refresh_cpu_all();
    sys.refresh_memory();
    SystemMetrics {
        cpu_usage: sys.global_cpu_usage(),
        memory_used_mb: sys.used_memory() / BYTES_PER_MB,
        memory_total_mb: sys.total_memory() / BYTES_PER_MB,
    }
}

#[cfg(not(feature = "sysinfo"))]
fn get_system_metrics() -> SystemMetrics {
    SystemMetrics {
        cpu_usage: 0.0,
        memory_used_mb: 0,
        memory_total_mb: 0,
    }
}

fn get_cert_expiry_days(cert_path: &str) -> Option<i64> {
    let pem = std::fs::read(cert_path).ok()?;
    let (_, pem_parsed) = x509_parser::pem::parse_x509_pem(&pem).ok()?;
    let (_, cert) = x509_parser::parse_x509_certificate(pem_parsed.contents.as_ref()).ok()?;
    let expiry_epoch = cert.validity().not_after.timestamp();
    let now_epoch = chrono::Utc::now().timestamp();
    Some((expiry_epoch - now_epoch) / SECS_PER_DAY)
}
