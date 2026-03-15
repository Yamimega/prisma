use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::MgmtState;

#[derive(Serialize)]
pub struct ClientBandwidthInfo {
    pub client_id: String,
    pub upload_bps: u64,
    pub download_bps: u64,
}

#[derive(Deserialize)]
pub struct UpdateBandwidthRequest {
    pub upload_bps: Option<u64>,
    pub download_bps: Option<u64>,
}

#[derive(Serialize)]
pub struct ClientQuotaInfo {
    pub client_id: String,
    pub quota_bytes: u64,
    pub used_bytes: u64,
    pub remaining_bytes: u64,
}

#[derive(Deserialize)]
pub struct UpdateQuotaRequest {
    pub quota_bytes: Option<u64>,
}

#[derive(Serialize)]
pub struct BandwidthSummary {
    pub clients: Vec<ClientBandwidthSummaryEntry>,
}

#[derive(Serialize)]
pub struct ClientBandwidthSummaryEntry {
    pub client_id: String,
    pub client_name: Option<String>,
    pub upload_bps: u64,
    pub download_bps: u64,
    pub quota_bytes: u64,
    pub quota_used: u64,
}

/// GET /api/clients/{id}/bandwidth
pub async fn get_client_bandwidth(
    State(state): State<MgmtState>,
    Path(id): Path<String>,
) -> Result<Json<ClientBandwidthInfo>, StatusCode> {
    let _bandwidth = state.bandwidth.as_ref().ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(ClientBandwidthInfo {
        client_id: id,
        upload_bps: 0,
        download_bps: 0,
    }))
}

/// PUT /api/clients/{id}/bandwidth
pub async fn update_client_bandwidth(
    State(state): State<MgmtState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateBandwidthRequest>,
) -> Result<Json<ClientBandwidthInfo>, StatusCode> {
    let bandwidth = state.bandwidth.as_ref().ok_or(StatusCode::NOT_FOUND)?;
    use prisma_core::bandwidth::limiter::BandwidthLimit;
    bandwidth
        .set_limit(
            &id,
            &BandwidthLimit {
                upload_bps: req.upload_bps.unwrap_or(0),
                download_bps: req.download_bps.unwrap_or(0),
            },
        )
        .await;
    Ok(Json(ClientBandwidthInfo {
        client_id: id,
        upload_bps: req.upload_bps.unwrap_or(0),
        download_bps: req.download_bps.unwrap_or(0),
    }))
}

/// GET /api/clients/{id}/quota
pub async fn get_client_quota(
    State(state): State<MgmtState>,
    Path(id): Path<String>,
) -> Result<Json<ClientQuotaInfo>, StatusCode> {
    let quotas = state.quotas.as_ref().ok_or(StatusCode::NOT_FOUND)?;
    if let Some(usage) = quotas.get(&id).await {
        Ok(Json(ClientQuotaInfo {
            client_id: id,
            quota_bytes: usage.quota_bytes,
            used_bytes: usage.total(),
            remaining_bytes: usage.remaining(),
        }))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

/// PUT /api/clients/{id}/quota
pub async fn update_client_quota(
    State(state): State<MgmtState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateQuotaRequest>,
) -> Result<StatusCode, StatusCode> {
    let quotas = state.quotas.as_ref().ok_or(StatusCode::NOT_FOUND)?;
    if let Some(bytes) = req.quota_bytes {
        quotas.set_quota(&id, bytes).await;
    }
    Ok(StatusCode::OK)
}

/// GET /api/bandwidth/summary
pub async fn get_bandwidth_summary(State(state): State<MgmtState>) -> Json<BandwidthSummary> {
    let mut clients = Vec::new();
    let auth = state.auth_store.read().await;

    for (id, entry) in &auth.clients {
        let id_str = id.to_string();
        let (quota_bytes, quota_used) = match &state.quotas {
            Some(quotas) => match quotas.get(&id_str).await {
                Some(usage) => (usage.quota_bytes, usage.total()),
                None => (0, 0),
            },
            None => (0, 0),
        };

        clients.push(ClientBandwidthSummaryEntry {
            client_id: id_str,
            client_name: entry.name.clone(),
            upload_bps: 0,
            download_bps: 0,
            quota_bytes,
            quota_used,
        });
    }

    Json(BandwidthSummary { clients })
}
