//! HTTP/3 masquerade for the QUIC listener.
//!
//! When camouflage is enabled with `h3_cover_site` or `h3_static_dir`, the QUIC
//! endpoint accepts connections and inspects the negotiated ALPN protocol.
//!
//! - If the ALPN is a PrismaVeil protocol (`prisma-v3`, `prisma-v2`, `prisma-v1`),
//!   the connection is handed off to `handler::handle_quic_stream` as usual.
//! - If the ALPN is `h3` (standard HTTP/3), the connection is treated as a browser
//!   or active prober and served a cover website over HTTP/3, making the server
//!   indistinguishable from a genuine HTTP/3 web server.

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;

use anyhow::Result;
use bytes::Bytes;
use h3::server::RequestStream;
use hyper::http::{self, Response, StatusCode};
use quinn::Endpoint;
use tokio::sync::Semaphore;
use tracing::{debug, info, warn};

use prisma_core::cache::DnsCache;
use prisma_core::config::server::ServerConfig;

use crate::auth::AuthStore;
use crate::handler;
use crate::state::ServerContext;

/// Run the H3 masquerade accept loop on an existing quinn `Endpoint`.
///
/// This replaces the normal QUIC accept loop when H3 masquerade is configured.
/// Each incoming QUIC connection is inspected: PrismaVeil clients are dispatched
/// to the tunnel handler while everything else gets HTTP/3 responses.
pub async fn accept_loop(
    endpoint: Endpoint,
    config: Arc<ServerConfig>,
    auth: AuthStore,
    dns_cache: DnsCache,
    ctx: ServerContext,
    semaphore: Arc<Semaphore>,
) {
    let cover = CoverConfig::from_server_config(&config);

    while let Some(incoming) = endpoint.accept().await {
        let auth = auth.clone();
        let dns = dns_cache.clone();
        let fwd = config.port_forwarding.clone();
        let ctx = ctx.clone();
        let semaphore = semaphore.clone();
        let cover = cover.clone();

        tokio::spawn(async move {
            let connection = match incoming.await {
                Ok(c) => c,
                Err(e) => {
                    warn!(error = %e, "H3 masquerade: failed to accept QUIC connection");
                    return;
                }
            };

            let remote = connection.remote_address();
            let alpn = connection
                .handshake_data()
                .and_then(|hd| hd.downcast::<quinn::crypto::rustls::HandshakeData>().ok())
                .and_then(|hd| hd.protocol)
                .unwrap_or_default();

            // If the negotiated ALPN is an H3 ALPN (h3 / h3-29), treat as HTTP/3.
            // PrismaVeil clients negotiate "prisma-v3", "prisma-v2", or "prisma-v1".
            let is_h3_alpn = alpn == b"h3" || alpn.starts_with(b"h3-");

            if is_h3_alpn {
                debug!(peer = %remote, "H3 masquerade: HTTP/3 client detected via ALPN");
                if let Err(e) = serve_h3_connection(connection, &cover).await {
                    debug!(peer = %remote, error = %e, "H3 masquerade: HTTP/3 session ended");
                }
                return;
            }

            // ALPN is a PrismaVeil protocol -- run normal tunnel handler.
            info!(peer = %remote, "H3 masquerade: PrismaVeil client detected via ALPN");
            loop {
                match connection.accept_bi().await {
                    Ok((send, recv)) => {
                        let permit = match semaphore.clone().try_acquire_owned() {
                            Ok(p) => p,
                            Err(_) => {
                                warn!(peer = %remote, "H3 masquerade: stream rejected (max connections)");
                                continue;
                            }
                        };
                        let auth = auth.clone();
                        let dns = dns.clone();
                        let fwd = fwd.clone();
                        let ctx = ctx.clone();
                        let peer_str = remote.to_string();
                        tokio::spawn(async move {
                            ctx.state
                                .metrics
                                .total_connections
                                .fetch_add(1, Ordering::Relaxed);
                            ctx.state
                                .metrics
                                .active_connections
                                .fetch_add(1, Ordering::Relaxed);
                            if let Err(e) = handler::handle_quic_stream(
                                send,
                                recv,
                                auth,
                                dns,
                                fwd,
                                ctx.clone(),
                                peer_str,
                            )
                            .await
                            {
                                warn!(error = %e, "H3 masquerade: QUIC stream handler error");
                            }
                            ctx.state
                                .metrics
                                .active_connections
                                .fetch_sub(1, Ordering::Relaxed);
                            drop(permit);
                        });
                    }
                    Err(quinn::ConnectionError::ApplicationClosed(_)) => break,
                    Err(e) => {
                        warn!(error = %e, "H3 masquerade: failed to accept QUIC stream");
                        break;
                    }
                }
            }
        });
    }
}

/// Configuration for H3 cover responses, derived from `CamouflageConfig`.
#[derive(Clone)]
struct CoverConfig {
    /// Upstream URL to reverse-proxy (e.g. "https://example.com").
    cover_site: Option<String>,
    /// Local directory of static files to serve.
    static_dir: Option<PathBuf>,
    /// Shared HTTP client for reverse-proxying to cover site (reused across requests).
    http_client: Arc<
        hyper_util::client::legacy::Client<
            hyper_util::client::legacy::connect::HttpConnector,
            http_body_util::Empty<Bytes>,
        >,
    >,
}

impl CoverConfig {
    fn from_server_config(config: &ServerConfig) -> Self {
        use hyper_util::client::legacy::Client;
        use hyper_util::rt::TokioExecutor;

        let http_client = Arc::new(
            Client::builder(TokioExecutor::new()).build_http::<http_body_util::Empty<Bytes>>(),
        );
        Self {
            cover_site: config.camouflage.h3_cover_site.clone(),
            static_dir: config.camouflage.h3_static_dir.as_ref().map(PathBuf::from),
            http_client,
        }
    }
}

/// Serve a single HTTP/3 connection using the h3 crate.
async fn serve_h3_connection(connection: quinn::Connection, cover: &CoverConfig) -> Result<()> {
    let h3_conn = h3_quinn::Connection::new(connection);

    let mut h3 = h3::server::Connection::new(h3_conn).await?;

    loop {
        match h3.accept().await {
            Ok(Some(resolver)) => {
                let cover = cover.clone();
                tokio::spawn(async move {
                    match resolver.resolve_request().await {
                        Ok((req, stream)) => {
                            if let Err(e) = handle_h3_request(req, stream, &cover).await {
                                debug!(error = %e, "H3 masquerade: request handler error");
                            }
                        }
                        Err(e) => {
                            debug!(error = %e, "H3 masquerade: failed to resolve request");
                        }
                    }
                });
            }
            Ok(None) => {
                // Connection closed gracefully.
                break;
            }
            Err(e) => {
                warn!(error = %e, "H3 masquerade: error accepting HTTP/3 request");
                break;
            }
        }
    }

    Ok(())
}

/// Handle a single HTTP/3 request by serving cover content.
async fn handle_h3_request(
    req: http::Request<()>,
    mut stream: RequestStream<h3_quinn::BidiStream<Bytes>, Bytes>,
    cover: &CoverConfig,
) -> Result<()> {
    let path = req.uri().path().to_string();
    debug!(method = %req.method(), path = %path, "H3 masquerade: serving cover request");

    // Try static directory first if configured.
    if let Some(ref dir) = cover.static_dir {
        let served = serve_static_file(&path, dir, &mut stream).await;
        if served {
            return Ok(());
        }
    }

    // Try reverse-proxying to cover site.
    if let Some(ref upstream) = cover.cover_site {
        if let Ok(()) = proxy_to_cover_site(&req, upstream, &mut stream, &cover.http_client).await {
            return Ok(());
        }
        // If proxy failed, fall through to default response.
        warn!("H3 masquerade: cover site proxy failed, serving default response");
    }

    // Default: return a simple HTML page so the prober sees a real HTTP/3 site.
    serve_default_page(&mut stream).await
}

/// Serve a static file from disk over the H3 stream.
///
/// Returns `true` if the file was found and served, `false` otherwise.
async fn serve_static_file(
    path: &str,
    dir: &Path,
    stream: &mut RequestStream<h3_quinn::BidiStream<Bytes>, Bytes>,
) -> bool {
    // Normalize the path: strip leading slash, default to index.html.
    let rel = path.trim_start_matches('/');
    let rel = if rel.is_empty() { "index.html" } else { rel };

    // Prevent path traversal.
    if rel.contains("..") {
        return false;
    }

    let file_path = dir.join(rel);

    let data = match tokio::fs::read(&file_path).await {
        Ok(d) => d,
        Err(_) => {
            // Try index.html fallback for directory-like paths.
            let index = dir.join("index.html");
            match tokio::fs::read(&index).await {
                Ok(d) => d,
                Err(_) => return false,
            }
        }
    };

    let content_type = guess_content_type(rel);

    let resp = Response::builder()
        .status(StatusCode::OK)
        .header("content-type", content_type)
        .header("server", "nginx")
        .header("alt-svc", "h3=\":443\"; ma=86400")
        .body(())
        .expect("valid response");

    if stream.send_response(resp).await.is_err() {
        return true; // Stream broken, but we did find the file.
    }
    let _ = stream.send_data(Bytes::from(data)).await;
    let _ = stream.finish().await;

    true
}

/// Proxy an H3 request to a cover upstream site via HTTP/1.1 and stream the
/// response back over H3.
async fn proxy_to_cover_site(
    req: &http::Request<()>,
    upstream: &str,
    stream: &mut RequestStream<h3_quinn::BidiStream<Bytes>, Bytes>,
    client: &hyper_util::client::legacy::Client<
        hyper_util::client::legacy::connect::HttpConnector,
        http_body_util::Empty<Bytes>,
    >,
) -> Result<()> {
    use http_body_util::BodyExt;

    let uri_string = format!(
        "{}{}",
        upstream.trim_end_matches('/'),
        req.uri()
            .path_and_query()
            .map(|pq: &http::uri::PathAndQuery| pq.as_str())
            .unwrap_or("/")
    );

    let uri: http::Uri = uri_string.parse()?;

    let upstream_req = http::Request::builder()
        .method(req.method())
        .uri(uri)
        .header("host", upstream_host(upstream))
        .header("user-agent", "Mozilla/5.0")
        .header("accept", "*/*")
        .body(http_body_util::Empty::<Bytes>::new())?;

    let resp = client.request(upstream_req).await?;

    let status = resp.status();
    let mut h3_resp = Response::builder().status(status);

    // Forward selected headers from the upstream response.
    for (name, value) in resp.headers() {
        let n = name.as_str();
        // Skip hop-by-hop and transfer-related headers.
        if matches!(
            n,
            "transfer-encoding" | "connection" | "keep-alive" | "upgrade"
        ) {
            continue;
        }
        h3_resp = h3_resp.header(name.clone(), value.clone());
    }

    // Ensure alt-svc advertises H3.
    h3_resp = h3_resp.header("alt-svc", "h3=\":443\"; ma=86400");

    let h3_resp = h3_resp.body(()).expect("valid response");
    stream.send_response(h3_resp).await?;

    // Stream the body.
    let mut body = resp.into_body();
    while let Some(frame) = body.frame().await {
        match frame {
            Ok(frame) => {
                if let Some(data) = frame.data_ref() {
                    stream.send_data(Bytes::copy_from_slice(data)).await?;
                }
            }
            Err(e) => {
                warn!(error = %e, "H3 masquerade: error reading upstream body");
                break;
            }
        }
    }

    stream.finish().await?;
    Ok(())
}

/// Serve a default "welcome" HTML page that looks like a real website.
async fn serve_default_page(
    stream: &mut RequestStream<h3_quinn::BidiStream<Bytes>, Bytes>,
) -> Result<()> {
    let body = concat!(
        "<!DOCTYPE html>\n",
        "<html><head><title>Welcome</title></head>\n",
        "<body><h1>Welcome</h1><p>This site is under construction.</p></body>\n",
        "</html>\n",
    );

    let resp = Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "text/html; charset=utf-8")
        .header("server", "nginx")
        .header("alt-svc", "h3=\":443\"; ma=86400")
        .body(())
        .expect("valid response");

    stream.send_response(resp).await?;
    stream.send_data(Bytes::from(body)).await?;
    stream.finish().await?;
    Ok(())
}

/// Extract the host portion from an upstream URL.
fn upstream_host(url: &str) -> String {
    url.trim_start_matches("http://")
        .trim_start_matches("https://")
        .split('/')
        .next()
        .unwrap_or("localhost")
        .to_string()
}

/// Guess a MIME content-type from a file extension.
fn guess_content_type(path: &str) -> &'static str {
    if path.ends_with(".html") || path.ends_with(".htm") {
        "text/html; charset=utf-8"
    } else if path.ends_with(".css") {
        "text/css; charset=utf-8"
    } else if path.ends_with(".js") {
        "application/javascript; charset=utf-8"
    } else if path.ends_with(".json") {
        "application/json"
    } else if path.ends_with(".png") {
        "image/png"
    } else if path.ends_with(".jpg") || path.ends_with(".jpeg") {
        "image/jpeg"
    } else if path.ends_with(".gif") {
        "image/gif"
    } else if path.ends_with(".svg") {
        "image/svg+xml"
    } else if path.ends_with(".ico") {
        "image/x-icon"
    } else if path.ends_with(".woff2") {
        "font/woff2"
    } else if path.ends_with(".woff") {
        "font/woff"
    } else if path.ends_with(".ttf") {
        "font/ttf"
    } else if path.ends_with(".xml") {
        "application/xml"
    } else if path.ends_with(".txt") {
        "text/plain; charset=utf-8"
    } else {
        "application/octet-stream"
    }
}
