use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Serialize;
use uuid::Uuid;

use prisma_core::state::{SessionMode, Transport};

use crate::MgmtState;

#[derive(Serialize)]
pub struct ConnectionResponse {
    pub session_id: Uuid,
    pub client_id: Option<Uuid>,
    pub client_name: Option<String>,
    pub peer_addr: String,
    pub transport: Transport,
    pub mode: SessionMode,
    pub connected_at: String,
    pub bytes_up: u64,
    pub bytes_down: u64,
}

pub async fn list(State(state): State<MgmtState>) -> Json<Vec<ConnectionResponse>> {
    let conns = state.connections.read().await;
    let list: Vec<_> = conns
        .values()
        .map(|c| ConnectionResponse {
            session_id: c.session_id,
            client_id: c.client_id,
            client_name: c.client_name.clone(),
            peer_addr: c.peer_addr.clone(),
            transport: c.transport,
            mode: c.mode,
            connected_at: c.connected_at.to_rfc3339(),
            bytes_up: c.bytes_up_val(),
            bytes_down: c.bytes_down_val(),
        })
        .collect();
    Json(list)
}

pub async fn disconnect(State(state): State<MgmtState>, Path(id): Path<Uuid>) -> StatusCode {
    let mut conns = state.connections.write().await;
    if conns.remove(&id).is_some() {
        StatusCode::OK
    } else {
        StatusCode::NOT_FOUND
    }
}
