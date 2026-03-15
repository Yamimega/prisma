use axum::extract::{Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};

use prisma_core::state::MetricsSnapshot;

use crate::MgmtState;

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub uptime_secs: u64,
    pub version: &'static str,
}

pub async fn health(State(state): State<MgmtState>) -> Json<HealthResponse> {
    let snapshot = state.snapshot_metrics();
    Json(HealthResponse {
        status: "ok",
        uptime_secs: snapshot.uptime_secs,
        version: env!("CARGO_PKG_VERSION"),
    })
}

pub async fn metrics(State(state): State<MgmtState>) -> Json<MetricsSnapshot> {
    Json(state.snapshot_metrics())
}

#[derive(Deserialize)]
pub struct HistoryParams {
    /// Period: "1h", "6h", "24h", "7d". Default: "1h".
    pub period: Option<String>,
    /// Resolution in seconds: "1", "10", "60". Default: "10".
    pub resolution: Option<String>,
}

/// Returns downsampled metrics history from the ring buffer.
pub async fn metrics_history(
    State(state): State<MgmtState>,
    Query(params): Query<HistoryParams>,
) -> Json<Vec<MetricsSnapshot>> {
    let period_secs: u64 = match params.period.as_deref() {
        Some("6h") => 6 * 3600,
        Some("24h") => 24 * 3600,
        Some("7d") => 7 * 24 * 3600,
        _ => 3600, // default 1h
    };

    let resolution_secs: u64 = match params.resolution.as_deref() {
        Some("1") | Some("1s") => 1,
        Some("60") | Some("60s") => 60,
        _ => 10, // default 10s
    };

    let history = state.metrics_history.read().await;
    let now = chrono::Utc::now();
    let cutoff = now - chrono::Duration::seconds(period_secs as i64);

    // Filter to the requested period
    let relevant: Vec<&MetricsSnapshot> =
        history.iter().filter(|s| s.timestamp >= cutoff).collect();

    if relevant.is_empty() {
        return Json(vec![]);
    }

    if resolution_secs <= 1 {
        // No downsampling needed
        return Json(relevant.into_iter().cloned().collect());
    }

    // Downsample by averaging within each resolution bucket
    let mut result = Vec::new();
    let mut bucket_start = relevant[0].timestamp;
    let mut bucket: Vec<&MetricsSnapshot> = Vec::new();

    for snapshot in &relevant {
        let elapsed = (snapshot.timestamp - bucket_start).num_seconds();
        if elapsed >= resolution_secs as i64 && !bucket.is_empty() {
            result.push(average_snapshots(&bucket));
            bucket.clear();
            bucket_start = snapshot.timestamp;
        }
        bucket.push(snapshot);
    }

    // Don't forget the last bucket
    if !bucket.is_empty() {
        result.push(average_snapshots(&bucket));
    }

    Json(result)
}

fn average_snapshots(snapshots: &[&MetricsSnapshot]) -> MetricsSnapshot {
    debug_assert!(
        !snapshots.is_empty(),
        "average_snapshots called with empty slice"
    );
    let len = snapshots.len() as u64;
    let last = snapshots.last().unwrap();
    MetricsSnapshot {
        timestamp: last.timestamp,
        uptime_secs: last.uptime_secs,
        total_connections: snapshots.iter().map(|s| s.total_connections).sum::<u64>() / len,
        active_connections: (snapshots
            .iter()
            .map(|s| s.active_connections as u64)
            .sum::<u64>()
            / len) as usize,
        total_bytes_up: last.total_bytes_up,
        total_bytes_down: last.total_bytes_down,
        handshake_failures: last.handshake_failures,
    }
}
