use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use prisma_core::state::ClientEntry;

use crate::MgmtState;

#[derive(Serialize)]
pub struct ClientResponse {
    pub id: Uuid,
    pub name: Option<String>,
    pub enabled: bool,
}

#[derive(Deserialize)]
pub struct CreateClientRequest {
    pub name: Option<String>,
}

#[derive(Serialize)]
pub struct CreateClientResponse {
    pub id: Uuid,
    pub name: Option<String>,
    pub auth_secret_hex: String,
}

#[derive(Deserialize)]
pub struct UpdateClientRequest {
    pub name: Option<String>,
    pub enabled: Option<bool>,
}

pub async fn list(State(state): State<MgmtState>) -> Json<Vec<ClientResponse>> {
    let store = state.auth_store.read().await;
    let clients: Vec<_> = store
        .clients
        .iter()
        .map(|(id, entry)| ClientResponse {
            id: *id,
            name: entry.name.clone(),
            enabled: entry.enabled,
        })
        .collect();
    Json(clients)
}

pub async fn create(
    State(state): State<MgmtState>,
    Json(req): Json<CreateClientRequest>,
) -> Result<Json<CreateClientResponse>, StatusCode> {
    let id = Uuid::new_v4();

    // Generate random auth secret
    let mut secret = [0u8; 32];
    rand::Rng::fill(&mut rand::thread_rng(), &mut secret);
    let hex = prisma_core::util::hex_encode(&secret);

    let entry = ClientEntry {
        auth_secret: secret,
        name: req.name.clone(),
        enabled: true,
    };

    state.auth_store.write().await.clients.insert(id, entry);

    Ok(Json(CreateClientResponse {
        id,
        name: req.name,
        auth_secret_hex: hex,
    }))
}

pub async fn update(
    State(state): State<MgmtState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateClientRequest>,
) -> StatusCode {
    let mut store = state.auth_store.write().await;
    match store.clients.get_mut(&id) {
        Some(entry) => {
            if let Some(name) = req.name {
                entry.name = Some(name);
            }
            if let Some(enabled) = req.enabled {
                entry.enabled = enabled;
            }
            StatusCode::OK
        }
        None => StatusCode::NOT_FOUND,
    }
}

pub async fn remove(State(state): State<MgmtState>, Path(id): Path<Uuid>) -> StatusCode {
    let mut store = state.auth_store.write().await;
    if store.clients.remove(&id).is_some() {
        StatusCode::OK
    } else {
        StatusCode::NOT_FOUND
    }
}
