use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;

use crate::AlertConfig;
use crate::MgmtState;

/// GET /api/alerts/config
pub async fn get_alert_config(State(state): State<MgmtState>) -> Json<AlertConfig> {
    Json(state.alert_config.read().await.clone())
}

/// PUT /api/alerts/config
pub async fn update_alert_config(
    State(state): State<MgmtState>,
    Json(config): Json<AlertConfig>,
) -> Result<Json<AlertConfig>, StatusCode> {
    *state.alert_config.write().await = config.clone();

    // Persist to alerts.json next to config
    if let Some(config_path) = &state.config_path {
        let alerts_path = config_path
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .join("alerts.json");
        let json =
            serde_json::to_string_pretty(&config).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        std::fs::write(alerts_path, json).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }

    Ok(Json(config))
}
