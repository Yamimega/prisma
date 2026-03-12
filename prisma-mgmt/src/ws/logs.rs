use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::Response;

use prisma_core::state::ServerState;

pub async fn ws_logs(
    ws: WebSocketUpgrade,
    State(state): State<ServerState>,
) -> Response {
    ws.on_upgrade(move |socket| handle_logs_ws(socket, state))
}

async fn handle_logs_ws(mut socket: WebSocket, state: ServerState) {
    let mut rx = state.log_tx.subscribe();
    let mut level_filter: Option<String> = None;
    let mut target_filter: Option<String> = None;

    loop {
        tokio::select! {
            result = rx.recv() => {
                match result {
                    Ok(entry) => {
                        // Apply client-side filters
                        if let Some(ref level) = level_filter {
                            if !entry.level.eq_ignore_ascii_case(level) {
                                continue;
                            }
                        }
                        if let Some(ref target) = target_filter {
                            if !entry.target.contains(target.as_str()) {
                                continue;
                            }
                        }

                        if let Ok(json) = serde_json::to_string(&entry) {
                            if socket.send(Message::Text(json.into())).await.is_err() {
                                break;
                            }
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => break,
                }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        // Client can send filter messages as JSON
                        if let Ok(filter) = serde_json::from_str::<serde_json::Value>(&text) {
                            if let Some(level) = filter.get("level").and_then(|v| v.as_str()) {
                                level_filter = if level.is_empty() { None } else { Some(level.to_string()) };
                            }
                            if let Some(target) = filter.get("target").and_then(|v| v.as_str()) {
                                target_filter = if target.is_empty() { None } else { Some(target.to_string()) };
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
        }
    }
}
