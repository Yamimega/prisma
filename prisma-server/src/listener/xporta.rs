use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use dashmap::DashMap;
use serde::Deserialize;
use tokio::sync::{mpsc, Mutex, Notify};
use tracing::{info, warn};
use uuid::Uuid;

use crate::handler;
use crate::listener::ws_tunnel::CdnState;
use crate::xporta_stream::XPortaServerStream;

use prisma_core::protocol::handshake::AuthVerifier;
use prisma_core::types::ClientId;
use prisma_core::util;
use prisma_core::xporta::encoding::{
    decode_request, encode_error, encode_poll_response, encode_response,
};
use prisma_core::xporta::reassembler::Reassembler;
use prisma_core::xporta::session::{create_cookie_token, verify_cookie_token};
use prisma_core::xporta::types::XPortaEncoding;

/// Shared state for XPorta handlers.
#[derive(Clone)]
pub struct XPortaState {
    pub cdn: CdnState,
    pub sessions: Arc<DashMap<[u8; 16], XPortaSession>>,
    pub cookie_key: [u8; 32],
    pub encoding: XPortaEncoding,
    pub cookie_name: String,
    pub session_timeout_secs: u64,
}

/// Per-session state held server-side.
pub struct XPortaSession {
    pub client_id: Uuid,
    /// Sends reassembled upload data to the XPortaServerStream reader.
    pub upload_tx: mpsc::Sender<Bytes>,
    /// Receives download data from XPortaServerStream writer.
    pub download_rx: Arc<Mutex<mpsc::Receiver<Bytes>>>,
    /// Parked poll waiters — each gets notified when download data is available.
    pub poll_notify: Arc<Notify>,
    /// Upload reassembler for ordering out-of-sequence upload chunks.
    pub upload_reassembler: Arc<Mutex<Reassembler>>,
    /// Monotonic download sequence counter.
    pub download_seq: AtomicU32,
    /// Unix timestamp of last activity.
    pub last_activity: AtomicU64,
}

use super::extract_peer_ip;

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Extract session cookie from request headers.
fn extract_cookie(headers: &HeaderMap, cookie_name: &str) -> Option<String> {
    for val in headers.get_all("cookie") {
        if let Ok(s) = val.to_str() {
            for part in s.split(';') {
                let part = part.trim();
                if let Some(value) = part.strip_prefix(cookie_name) {
                    let value = value.trim_start();
                    if let Some(value) = value.strip_prefix('=') {
                        return Some(value.trim().to_string());
                    }
                }
            }
        }
    }
    None
}

/// Validate session cookie and return session_id.
fn validate_session(state: &XPortaState, headers: &HeaderMap) -> Option<[u8; 16]> {
    let token = extract_cookie(headers, &state.cookie_name)?;

    // We need to try against all known client IDs in the session
    // Since the token includes session_id, we verify it and look up the session
    // The client_id is embedded in the session, so we try to decode first and then verify

    // Parse the token to extract session_id, then look up client_id from session
    let token_bytes = util::hex_decode(&token)?;
    if token_bytes.len() != 56 {
        return None;
    }

    let mut session_id = [0u8; 16];
    session_id.copy_from_slice(&token_bytes[0..16]);

    // Look up session to get client_id
    let session = state.sessions.get(&session_id)?;
    let client_uuid = session.client_id;
    drop(session);

    // Now verify the full token with the client_id
    let client_id_bytes = *client_uuid.as_bytes();
    let result = verify_cookie_token(&state.cookie_key, &token, &client_id_bytes, now_secs())?;

    // Update last activity
    if let Some(session) = state.sessions.get(&result.0) {
        session.last_activity.store(now_secs(), Ordering::Relaxed);
    }

    Some(result.0)
}

/// POST /api/auth — Session initialization.
/// Validates client credentials and returns a Set-Cookie header.
pub async fn session_init_handler(
    State(state): State<XPortaState>,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<std::net::SocketAddr>,
    headers: HeaderMap,
    body: Body,
) -> impl IntoResponse {
    let peer_ip = extract_peer_ip(&headers, &addr);

    // Read body
    let body_bytes = match axum::body::to_bytes(body, 4096).await {
        Ok(b) => b,
        Err(_) => {
            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .header("content-type", "application/json")
                .body(Body::from(encode_error("invalid request", 400)))
                .unwrap()
                .into_response();
        }
    };

    // Parse session init request
    let req: prisma_core::xporta::types::SessionInitRequest =
        match serde_json::from_slice(&body_bytes) {
            Ok(r) => r,
            Err(_) => {
                return Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .header("content-type", "application/json")
                    .body(Body::from(encode_error("invalid request", 400)))
                    .unwrap()
                    .into_response();
            }
        };

    // Parse client_id
    let client_uuid = match Uuid::parse_str(&req.c) {
        Ok(u) => u,
        Err(_) => {
            return Response::builder()
                .status(StatusCode::UNAUTHORIZED)
                .header("content-type", "application/json")
                .body(Body::from(encode_error("unauthorized", 401)))
                .unwrap()
                .into_response();
        }
    };

    let client_id = ClientId::from_uuid(client_uuid);

    // Parse auth token
    let auth_token_bytes = match util::hex_decode(&req.a) {
        Some(b) if b.len() == 32 => {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&b);
            arr
        }
        _ => {
            return Response::builder()
                .status(StatusCode::UNAUTHORIZED)
                .header("content-type", "application/json")
                .body(Body::from(encode_error("unauthorized", 401)))
                .unwrap()
                .into_response();
        }
    };

    // Verify auth
    if !state.cdn.auth.verify(&client_id, &auth_token_bytes, req.t) {
        return Response::builder()
            .status(StatusCode::UNAUTHORIZED)
            .header("content-type", "application/json")
            .body(Body::from(encode_error("unauthorized", 401)))
            .unwrap()
            .into_response();
    }

    // Generate session
    let mut session_id = [0u8; 16];
    rand::Rng::fill(&mut rand::thread_rng(), &mut session_id);

    let expiry = now_secs() + state.session_timeout_secs;
    let client_id_bytes = *client_uuid.as_bytes();
    let token = create_cookie_token(&state.cookie_key, &session_id, &client_id_bytes, expiry);

    // Create session channels
    let (upload_tx, upload_rx) = mpsc::channel::<Bytes>(256);
    let (download_tx, download_rx) = mpsc::channel::<Bytes>(256);

    let session = XPortaSession {
        client_id: client_uuid,
        upload_tx,
        download_rx: Arc::new(Mutex::new(download_rx)),
        poll_notify: Arc::new(Notify::new()),
        upload_reassembler: Arc::new(Mutex::new(Reassembler::new())),
        download_seq: AtomicU32::new(0),
        last_activity: AtomicU64::new(now_secs()),
    };

    state.sessions.insert(session_id, session);

    // Spawn the PrismaVeil protocol handler with XPortaServerStream
    let xporta_stream = XPortaServerStream::new(upload_rx, download_tx);
    let cdn = state.cdn.clone();
    let sessions = state.sessions.clone();
    let sid = session_id;

    tokio::spawn(async move {
        cdn.ctx
            .state
            .metrics
            .total_connections
            .fetch_add(1, Ordering::Relaxed);
        cdn.ctx
            .state
            .metrics
            .active_connections
            .fetch_add(1, Ordering::Relaxed);

        info!(peer = %peer_ip, session = ?util::hex_encode(&sid), "XPorta session started");

        let fwd = cdn.config.port_forwarding.clone();
        let result = handler::handle_tcp_connection_camouflaged(
            xporta_stream,
            cdn.auth.clone(),
            cdn.dns.clone(),
            fwd,
            cdn.ctx.clone(),
            peer_ip.clone(),
            None,
        )
        .await;

        if let Err(e) = result {
            warn!(peer = %peer_ip, session = ?util::hex_encode(&sid), error = %e, "XPorta session error");
        }

        cdn.ctx
            .state
            .metrics
            .active_connections
            .fetch_sub(1, Ordering::Relaxed);

        sessions.remove(&sid);
    });

    // Return Set-Cookie
    let cookie_val = format!(
        "{}={}; HttpOnly; Secure; SameSite=Strict; Path=/",
        state.cookie_name, token
    );

    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/json")
        .header("set-cookie", cookie_val)
        .body(Body::from(b"{\"ok\":true}".to_vec()))
        .unwrap()
        .into_response()
}

/// POST /api/v1/data (or other data_paths) — Upload handler.
/// Receives upload data, sends it to reassembler, optionally piggybacks download data.
pub async fn upload_handler(
    State(state): State<XPortaState>,
    headers: HeaderMap,
    body: Body,
) -> impl IntoResponse {
    // Validate session
    let session_id = match validate_session(&state, &headers) {
        Some(sid) => sid,
        None => {
            return Response::builder()
                .status(StatusCode::UNAUTHORIZED)
                .header("content-type", "application/json")
                .body(Body::from(encode_error("unauthorized", 401)))
                .unwrap()
                .into_response();
        }
    };

    // Read body
    let body_bytes = match axum::body::to_bytes(body, 128 * 1024).await {
        Ok(b) => b,
        Err(_) => {
            return StatusCode::BAD_REQUEST.into_response();
        }
    };

    // Decode the upload request
    let (seq, payload) = match decode_request(&body_bytes, state.encoding) {
        Some(r) => r,
        None => {
            return StatusCode::BAD_REQUEST.into_response();
        }
    };

    // Get session — extract Arc handles immediately, drop guard before awaiting
    let (upload_tx, upload_reassembler, download_rx) = match state.sessions.get(&session_id) {
        Some(s) => (
            s.upload_tx.clone(),
            s.upload_reassembler.clone(),
            s.download_rx.clone(),
        ),
        None => {
            return Response::builder()
                .status(StatusCode::UNAUTHORIZED)
                .header("content-type", "application/json")
                .body(Body::from(encode_error("unauthorized", 401)))
                .unwrap()
                .into_response();
        }
    };

    // Insert into reassembler
    {
        let mut reassembler = upload_reassembler.lock().await;
        if let Err(e) = reassembler.insert(seq, payload) {
            warn!(error = %e, "XPorta reassembler error");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }

        // Drain in-order data and send to upload channel
        for chunk in reassembler.drain() {
            if upload_tx.send(Bytes::from(chunk)).await.is_err() {
                return StatusCode::GONE.into_response();
            }
        }
    }

    // Try to piggyback download data
    let mut dl_seq = None;
    let mut dl_data = None;
    {
        let mut rx = download_rx.lock().await;
        if let Ok(data) = rx.try_recv() {
            // Re-check session for sequence counter
            if let Some(session) = state.sessions.get(&session_id) {
                let s = session.download_seq.fetch_add(1, Ordering::Relaxed);
                dl_seq = Some(s);
                dl_data = Some(data);
            }
        }
    }

    let response_body = encode_response(dl_seq, dl_data.as_deref(), state.encoding);

    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", state.encoding.content_type())
        .header("cache-control", "no-cache, no-store")
        .body(Body::from(response_body))
        .unwrap()
        .into_response()
}

/// Query params for poll requests.
#[derive(Deserialize)]
pub struct PollQuery {
    #[allow(dead_code)]
    pub since: Option<u32>,
    #[allow(dead_code)]
    pub _t: Option<String>,
}

/// GET /api/v1/notifications (or other poll_paths) — Long-poll download handler.
/// Holds the request for up to 55 seconds, responds with batched download items.
pub async fn poll_handler(
    State(state): State<XPortaState>,
    headers: HeaderMap,
    Query(_params): Query<PollQuery>,
) -> impl IntoResponse {
    // Validate session
    let session_id = match validate_session(&state, &headers) {
        Some(sid) => sid,
        None => {
            return Response::builder()
                .status(StatusCode::UNAUTHORIZED)
                .header("content-type", "application/json")
                .body(Body::from(encode_error("unauthorized", 401)))
                .unwrap()
                .into_response();
        }
    };

    // Get session
    let download_rx = match state.sessions.get(&session_id) {
        Some(s) => s.download_rx.clone(),
        None => {
            return Response::builder()
                .status(StatusCode::UNAUTHORIZED)
                .header("content-type", "application/json")
                .body(Body::from(encode_error("unauthorized", 401)))
                .unwrap()
                .into_response();
        }
    };

    // Try to collect download data with timeout
    let mut items: Vec<(u32, Bytes)> = Vec::new();

    {
        let mut rx = download_rx.lock().await;

        // First, try non-blocking recv for any immediately available data
        while let Ok(data) = rx.try_recv() {
            if let Some(session) = state.sessions.get(&session_id) {
                let seq = session.download_seq.fetch_add(1, Ordering::Relaxed);
                items.push((seq, data));
            }
            if items.len() >= 16 {
                break; // Batch limit
            }
        }

        // If no immediate data, wait with timeout
        if items.is_empty() {
            let timeout = tokio::time::Duration::from_secs(55);
            match tokio::time::timeout(timeout, rx.recv()).await {
                Ok(Some(data)) => {
                    if let Some(session) = state.sessions.get(&session_id) {
                        let seq = session.download_seq.fetch_add(1, Ordering::Relaxed);
                        items.push((seq, data));
                    }
                    // Drain any more immediately available
                    while let Ok(data) = rx.try_recv() {
                        if let Some(session) = state.sessions.get(&session_id) {
                            let seq = session.download_seq.fetch_add(1, Ordering::Relaxed);
                            items.push((seq, data));
                        }
                        if items.len() >= 16 {
                            break;
                        }
                    }
                }
                Ok(None) => {
                    // Channel closed — session ended
                }
                Err(_) => {
                    // Timeout — return empty items (normal)
                }
            }
        }
    }

    let items_refs: Vec<(u32, &[u8])> = items.iter().map(|(s, d)| (*s, d.as_ref())).collect();
    let response_body = encode_poll_response(&items_refs);

    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/json")
        .header("cache-control", "no-cache, no-store")
        .body(Body::from(response_body))
        .unwrap()
        .into_response()
}

/// Spawn a background task that periodically cleans up idle sessions.
pub fn spawn_session_cleanup(sessions: Arc<DashMap<[u8; 16], XPortaSession>>, timeout_secs: u64) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(60));
        loop {
            interval.tick().await;
            let now = now_secs();
            let mut to_remove = Vec::new();
            for entry in sessions.iter() {
                let last = entry.value().last_activity.load(Ordering::Relaxed);
                if now - last > timeout_secs {
                    to_remove.push(*entry.key());
                }
            }
            for sid in to_remove {
                sessions.remove(&sid);
                info!(session = ?util::hex_encode(&sid), "XPorta session expired");
            }
        }
    });
}
