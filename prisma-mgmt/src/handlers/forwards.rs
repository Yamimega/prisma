use axum::extract::State;
use axum::Json;
use serde::Serialize;
use uuid::Uuid;

use crate::MgmtState;

#[derive(Serialize)]
pub struct ForwardInfo {
    pub session_id: Uuid,
    pub peer_addr: String,
    pub connected_at: String,
    pub bytes_up: u64,
    pub bytes_down: u64,
}

pub async fn list(State(state): State<MgmtState>) -> Json<Vec<ForwardInfo>> {
    let conns = state.connections.read().await;
    let forwards: Vec<_> = conns
        .values()
        .filter(|c| c.mode == prisma_core::state::SessionMode::Forward)
        .map(|c| ForwardInfo {
            session_id: c.session_id,
            peer_addr: c.peer_addr.clone(),
            connected_at: c.connected_at.to_rfc3339(),
            bytes_up: c.bytes_up_val(),
            bytes_down: c.bytes_down_val(),
        })
        .collect();
    Json(forwards)
}
