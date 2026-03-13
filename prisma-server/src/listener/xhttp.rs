use std::sync::atomic::Ordering;

use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use dashmap::DashMap;
use futures_util::StreamExt;
use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::handler;
use crate::listener::ws_tunnel::CdnState;
use crate::xhttp_stream::XhttpStream;

/// Session state for packet-up mode: correlates upload POSTs with download GETs.
pub type SessionMap = DashMap<String, SessionChannels>;

pub struct SessionChannels {
    /// Send data from upload handler to the XhttpStream reader
    pub upload_tx: mpsc::Sender<Bytes>,
    /// Receive data from XhttpStream writer for download handler
    pub download_rx: mpsc::Receiver<Bytes>,
}

/// Shared state for XHTTP handlers, wrapping CdnState + session map.
#[derive(Clone)]
pub struct XhttpState {
    pub cdn: CdnState,
    pub sessions: std::sync::Arc<SessionMap>,
}

/// Extract real client IP from headers (reuses CDN logic).
fn extract_peer_ip(headers: &HeaderMap, addr: &std::net::SocketAddr) -> String {
    // CF-Connecting-IP > X-Real-IP > X-Forwarded-For > socket addr
    if let Some(val) = headers.get("cf-connecting-ip") {
        if let Ok(ip) = val.to_str() {
            return ip.to_string();
        }
    }
    if let Some(val) = headers.get("x-real-ip") {
        if let Ok(ip) = val.to_str() {
            return ip.to_string();
        }
    }
    if let Some(val) = headers.get("x-forwarded-for") {
        if let Ok(s) = val.to_str() {
            if let Some(first) = s.split(',').next() {
                return first.trim().to_string();
            }
        }
    }
    addr.to_string()
}

/// POST /upload-path — packet-up mode: receives chunked body, each chunk is a PrismaVeil frame.
/// Creates a session on first POST (identified by X-Session-ID header).
pub async fn packet_upload_handler(
    State(state): State<XhttpState>,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<std::net::SocketAddr>,
    headers: HeaderMap,
    body: Body,
) -> impl IntoResponse {
    let session_id = headers
        .get("x-session-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    if session_id.is_empty() {
        return StatusCode::BAD_REQUEST.into_response();
    }

    let peer_ip = extract_peer_ip(&headers, &addr);

    // If session doesn't exist yet, create it and spawn handler
    if !state.sessions.contains_key(&session_id) {
        let (upload_tx, upload_rx) = mpsc::channel::<Bytes>(256);
        let (download_tx, download_rx) = mpsc::channel::<Bytes>(256);

        state.sessions.insert(
            session_id.clone(),
            SessionChannels {
                upload_tx: upload_tx.clone(),
                download_rx,
            },
        );

        let xhttp_stream = XhttpStream::new(upload_rx, download_tx);
        let cdn = state.cdn.clone();
        let sid = session_id.clone();
        let sessions = state.sessions.clone();

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

            info!(peer = %peer_ip, session = %sid, "XHTTP packet-up session started");

            let fwd = cdn.config.port_forwarding.clone();
            let result = handler::handle_tcp_connection_camouflaged(
                xhttp_stream,
                cdn.auth.clone(),
                cdn.dns.clone(),
                fwd,
                cdn.ctx.clone(),
                peer_ip.clone(),
                None,
            )
            .await;

            if let Err(e) = result {
                warn!(peer = %peer_ip, session = %sid, error = %e, "XHTTP packet-up error");
            }

            cdn.ctx
                .state
                .metrics
                .active_connections
                .fetch_sub(1, Ordering::Relaxed);
            sessions.remove(&sid);
        });

        // Feed the first body chunk via the upload_tx we just created
        feed_body_to_channel(body, upload_tx).await;
    } else {
        // Existing session — feed body data to upload channel
        if let Some(entry) = state.sessions.get(&session_id) {
            let tx = entry.upload_tx.clone();
            drop(entry);
            feed_body_to_channel(body, tx).await;
        }
    }

    StatusCode::OK.into_response()
}

/// GET /download-path — packet-up mode: long-running response that streams data back.
pub async fn packet_download_handler(
    State(state): State<XhttpState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let session_id = headers
        .get("x-session-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    if session_id.is_empty() {
        return StatusCode::BAD_REQUEST.into_response();
    }

    // Take the download_rx from the session
    let download_rx = match state.sessions.remove(&session_id) {
        Some((sid, channels)) => {
            // Re-insert without download_rx (it's been consumed)
            // We keep only upload_tx for future uploads
            state.sessions.insert(
                sid,
                SessionChannels {
                    upload_tx: channels.upload_tx,
                    download_rx: mpsc::channel(1).1, // dummy — already taken
                },
            );
            channels.download_rx
        }
        None => {
            return StatusCode::NOT_FOUND.into_response();
        }
    };

    let use_sse = !state.cdn.config.cdn.xhttp_nosse;

    let stream = tokio_stream::wrappers::ReceiverStream::new(download_rx).map(move |data| {
        if use_sse {
            // Wrap in SSE format: data:<base64>\n\n (or binary)
            let mut frame = Vec::with_capacity(6 + data.len() + 2);
            frame.extend_from_slice(b"data:");
            frame.extend_from_slice(&data);
            frame.extend_from_slice(b"\n\n");
            Ok::<_, std::convert::Infallible>(frame.into())
        } else {
            Ok::<_, std::convert::Infallible>(data)
        }
    });

    let content_type = if use_sse {
        "text/event-stream"
    } else {
        "application/octet-stream"
    };

    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", content_type)
        .header("cache-control", "no-cache")
        .header("connection", "keep-alive")
        .body(Body::from_stream(stream))
        .unwrap()
        .into_response()
}

/// POST /stream-path — stream-one/stream-up mode: bidirectional HTTP/2 streaming.
pub async fn stream_handler(
    State(state): State<XhttpState>,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<std::net::SocketAddr>,
    headers: HeaderMap,
    body: Body,
) -> impl IntoResponse {
    let peer_ip = extract_peer_ip(&headers, &addr);

    let (upload_tx, upload_rx) = mpsc::channel::<Bytes>(256);
    let (download_tx, download_rx) = mpsc::channel::<Bytes>(256);

    let xhttp_stream = XhttpStream::new(upload_rx, download_tx);
    let cdn = state.cdn.clone();

    // Spawn the handler task
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

        info!(peer = %peer_ip, "XHTTP stream session started");

        let fwd = cdn.config.port_forwarding.clone();
        let result = handler::handle_tcp_connection_camouflaged(
            xhttp_stream,
            cdn.auth.clone(),
            cdn.dns.clone(),
            fwd,
            cdn.ctx.clone(),
            peer_ip.clone(),
            None,
        )
        .await;

        if let Err(e) = result {
            warn!(peer = %peer_ip, error = %e, "XHTTP stream error");
        }

        cdn.ctx
            .state
            .metrics
            .active_connections
            .fetch_sub(1, Ordering::Relaxed);
    });

    // Feed request body to upload channel
    tokio::spawn(async move {
        feed_body_to_channel(body, upload_tx).await;
    });

    // Stream response body from download channel
    let stream = tokio_stream::wrappers::ReceiverStream::new(download_rx)
        .map(Ok::<_, std::convert::Infallible>);

    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/octet-stream")
        .header("cache-control", "no-cache")
        .body(Body::from_stream(stream))
        .unwrap()
        .into_response()
}

/// Helper: read all chunks from an axum Body and send them to a channel.
async fn feed_body_to_channel(body: Body, tx: mpsc::Sender<Bytes>) {
    let mut stream = body.into_data_stream();
    while let Some(chunk) = stream.next().await {
        match chunk {
            Ok(data) => {
                if tx.send(data).await.is_err() {
                    break;
                }
            }
            Err(_) => break,
        }
    }
}
