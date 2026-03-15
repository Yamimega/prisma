use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use bytes::Bytes;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::sync::{mpsc, Mutex, Semaphore};
use tokio::time::Duration;
use tracing::{debug, warn};

use prisma_core::util;
use prisma_core::xporta::encoding::{
    decode_poll_response, decode_response, encode_request, encode_session_init,
};
use prisma_core::xporta::reassembler::Reassembler;
use prisma_core::xporta::types::XPortaEncoding;

type ReserveFut<T> = Pin<
    Box<dyn Future<Output = Result<mpsc::OwnedPermit<T>, mpsc::error::SendError<()>>> + Send>,
>;

/// Client-side XPorta stream -- provides AsyncRead + AsyncWrite over multiple short HTTP requests.
pub struct XPortaClientStream {
    /// Receives in-order reassembled download data.
    read_rx: mpsc::Receiver<Bytes>,
    /// Sends upload data to the upload dispatcher.
    write_tx: mpsc::Sender<Bytes>,
    read_buf: Vec<u8>,
    read_pos: usize,
    write_reserve: Option<ReserveFut<Bytes>>,
}

impl XPortaClientStream {
    pub fn new(read_rx: mpsc::Receiver<Bytes>, write_tx: mpsc::Sender<Bytes>) -> Self {
        Self {
            read_rx,
            write_tx,
            read_buf: Vec::new(),
            read_pos: 0,
            write_reserve: None,
        }
    }
}

impl AsyncRead for XPortaClientStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let this = self.get_mut();

        if this.read_pos < this.read_buf.len() {
            let remaining = &this.read_buf[this.read_pos..];
            let to_copy = remaining.len().min(buf.remaining());
            buf.put_slice(&remaining[..to_copy]);
            this.read_pos += to_copy;
            if this.read_pos >= this.read_buf.len() {
                this.read_buf.clear();
                this.read_pos = 0;
            }
            return Poll::Ready(Ok(()));
        }

        match this.read_rx.poll_recv(cx) {
            Poll::Ready(Some(data)) => {
                let to_copy = data.len().min(buf.remaining());
                buf.put_slice(&data[..to_copy]);
                if to_copy < data.len() {
                    this.read_buf = data[to_copy..].to_vec();
                    this.read_pos = 0;
                }
                Poll::Ready(Ok(()))
            }
            Poll::Ready(None) => Poll::Ready(Ok(())),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl AsyncWrite for XPortaClientStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let this = self.get_mut();

        let mut fut = this
            .write_reserve
            .take()
            .unwrap_or_else(|| Box::pin(this.write_tx.clone().reserve_owned()));

        match fut.as_mut().poll(cx) {
            Poll::Ready(Ok(permit)) => {
                permit.send(Bytes::copy_from_slice(buf));
                Poll::Ready(Ok(buf.len()))
            }
            Poll::Ready(Err(_)) => Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "channel closed",
            ))),
            Poll::Pending => {
                this.write_reserve = Some(fut);
                Poll::Pending
            }
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

/// Configuration for creating an XPorta connection.
pub struct XPortaConfig {
    pub base_url: String,
    pub session_path: String,
    pub data_paths: Vec<String>,
    pub poll_paths: Vec<String>,
    pub encoding: XPortaEncoding,
    pub poll_concurrency: u8,
    pub upload_concurrency: u8,
    pub max_payload_size: u32,
    pub poll_timeout_secs: u16,
    pub extra_headers: Vec<(String, String)>,
    pub user_agent: Option<String>,
    pub referer: Option<String>,
    pub cookie_name: String,
}

/// Connect to the server via XPorta transport.
///
/// 1. Establish TLS+H2 connection
/// 2. Send session init POST
/// 3. Extract session cookie from response
/// 4. Spawn upload dispatcher, poll pool, and download reassembler tasks
/// 5. Return XPortaClientStream
pub async fn connect_xporta(
    config: &XPortaConfig,
    client_id_hex: &str,
    auth_secret: &[u8; 32],
) -> Result<XPortaClientStream> {
    let uri: http::Uri = config
        .base_url
        .parse()
        .map_err(|e| anyhow::anyhow!("Invalid base_url: {}", e))?;

    let host = uri.host().unwrap_or("localhost").to_string();
    let port = uri.port_u16().unwrap_or(443);
    let is_tls = uri.scheme_str() == Some("https");
    let addr = format!("{}:{}", host, port);

    // Connect TCP
    let tcp_stream = tokio::net::TcpStream::connect(&addr).await?;

    // Wrap in TLS + H2
    let tls_config = crate::connector::build_client_tls_config(true, &["h2".to_string()]);
    let connector = tokio_rustls::TlsConnector::from(Arc::new(tls_config));

    let (h2_sender, session_cookie) = if is_tls {
        let sni = rustls::pki_types::ServerName::try_from(host.clone())?;
        let tls_stream = connector.connect(sni, tcp_stream).await?;
        let (sender, conn) = hyper::client::conn::http2::handshake(
            hyper_util::rt::TokioExecutor::new(),
            hyper_util::rt::TokioIo::new(tls_stream),
        )
        .await
        .map_err(|e| anyhow::anyhow!("H2 handshake failed: {}", e))?;

        // Drive connection in background
        tokio::spawn(async move {
            if let Err(e) = conn.await {
                warn!("XPorta H2 connection error: {}", e);
            }
        });

        // Session init
        let cookie = session_init(&sender, config, client_id_hex, auth_secret, &host).await?;
        (sender, cookie)
    } else {
        let (sender, conn) = hyper::client::conn::http2::handshake(
            hyper_util::rt::TokioExecutor::new(),
            hyper_util::rt::TokioIo::new(tcp_stream),
        )
        .await
        .map_err(|e| anyhow::anyhow!("H2 handshake failed: {}", e))?;

        tokio::spawn(async move {
            if let Err(e) = conn.await {
                warn!("XPorta H2 connection error: {}", e);
            }
        });

        let cookie = session_init(&sender, config, client_id_hex, auth_secret, &host).await?;
        (sender, cookie)
    };

    debug!(
        cookie_len = session_cookie.len(),
        "XPorta session established"
    );

    // Create channels
    let (upload_tx, upload_rx) = mpsc::channel::<Bytes>(256);
    let (download_tx, download_rx) = mpsc::channel::<Bytes>(256);

    let upload_seq = Arc::new(AtomicU32::new(0));
    let dl_reassembler = Arc::new(Mutex::new(Reassembler::new()));

    // Spawn upload dispatcher
    {
        let sender = h2_sender.clone();
        let encoding = config.encoding;
        let data_paths = config.data_paths.clone();
        let cookie_header = format!("{}={}", config.cookie_name, session_cookie);
        let host = host.clone();
        let upload_sem = Arc::new(Semaphore::new(config.upload_concurrency as usize));
        let extra_headers = config.extra_headers.clone();
        let user_agent = config.user_agent.clone();
        let dl_reassembler = dl_reassembler.clone();
        let download_tx = download_tx.clone();
        let max_payload = config.max_payload_size as usize;

        tokio::spawn(async move {
            upload_dispatcher(
                upload_rx,
                sender,
                upload_seq,
                encoding,
                data_paths,
                cookie_header,
                host,
                upload_sem,
                extra_headers,
                user_agent,
                dl_reassembler,
                download_tx,
                max_payload,
            )
            .await;
        });
    }

    // Spawn poll pool
    {
        let sender = h2_sender.clone();
        let poll_paths = config.poll_paths.clone();
        let cookie_header = format!("{}={}", config.cookie_name, session_cookie);
        let host = host.clone();
        let poll_concurrency = config.poll_concurrency;
        let extra_headers = config.extra_headers.clone();
        let user_agent = config.user_agent.clone();
        let dl_reassembler = dl_reassembler.clone();
        let download_tx = download_tx.clone();

        for _ in 0..poll_concurrency {
            let sender = sender.clone();
            let poll_paths = poll_paths.clone();
            let cookie_header = cookie_header.clone();
            let host = host.clone();
            let extra_headers = extra_headers.clone();
            let user_agent = user_agent.clone();
            let dl_reassembler = dl_reassembler.clone();
            let download_tx = download_tx.clone();

            tokio::spawn(async move {
                poll_loop(
                    sender,
                    poll_paths,
                    cookie_header,
                    host,
                    extra_headers,
                    user_agent,
                    dl_reassembler,
                    download_tx,
                )
                .await;
            });
        }
    }

    Ok(XPortaClientStream::new(download_rx, upload_tx))
}

/// Perform session initialization POST.
async fn session_init(
    sender: &hyper::client::conn::http2::SendRequest<http_body_util::Full<Bytes>>,
    config: &XPortaConfig,
    client_id_hex: &str,
    auth_secret: &[u8; 32],
    host: &str,
) -> Result<String> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let client_id = prisma_core::types::ClientId::from_uuid(
        uuid::Uuid::parse_str(client_id_hex)
            .map_err(|e| anyhow::anyhow!("Invalid client_id: {}", e))?,
    );
    let auth_token = util::compute_auth_token(auth_secret, &client_id, timestamp);
    let auth_token_hex = util::hex_encode(&auth_token);

    let body_bytes = encode_session_init(client_id_hex, &auth_token_hex, timestamp);

    let url = format!("{}{}", config.base_url, config.session_path);

    let mut req_builder = http::Request::builder()
        .method("POST")
        .uri(&url)
        .header("content-type", "application/json")
        .header("host", host);

    if let Some(ref ua) = config.user_agent {
        req_builder = req_builder.header("user-agent", ua.as_str());
    } else {
        req_builder = req_builder.header(
            "user-agent",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
        );
    }

    for (k, v) in &config.extra_headers {
        req_builder = req_builder.header(k.as_str(), v.as_str());
    }

    let req = req_builder
        .body(http_body_util::Full::new(Bytes::from(body_bytes)))
        .map_err(|e| anyhow::anyhow!("Failed to build session init request: {}", e))?;

    let mut sender = sender.clone();
    let response = sender
        .send_request(req)
        .await
        .map_err(|e| anyhow::anyhow!("Session init request failed: {}", e))?;

    if response.status() != http::StatusCode::OK {
        return Err(anyhow::anyhow!(
            "Session init failed with status {}",
            response.status()
        ));
    }

    // Extract session cookie from Set-Cookie header
    let cookie_name = &config.cookie_name;
    for val in response.headers().get_all("set-cookie") {
        if let Ok(s) = val.to_str() {
            for part in s.split(';') {
                let part = part.trim();
                if let Some(value) = part.strip_prefix(cookie_name.as_str()) {
                    let value = value.trim_start();
                    if let Some(value) = value.strip_prefix('=') {
                        return Ok(value.trim().to_string());
                    }
                }
            }
        }
    }

    Err(anyhow::anyhow!("No session cookie in response"))
}

/// Upload dispatcher: reads from write channel, sends POST requests to random data paths.
#[allow(clippy::too_many_arguments)]
async fn upload_dispatcher(
    mut upload_rx: mpsc::Receiver<Bytes>,
    mut sender: hyper::client::conn::http2::SendRequest<http_body_util::Full<Bytes>>,
    upload_seq: Arc<AtomicU32>,
    encoding: XPortaEncoding,
    data_paths: Vec<String>,
    cookie_header: String,
    host: String,
    upload_sem: Arc<Semaphore>,
    extra_headers: Vec<(String, String)>,
    user_agent: Option<String>,
    dl_reassembler: Arc<Mutex<Reassembler>>,
    download_tx: mpsc::Sender<Bytes>,
    max_payload: usize,
) {
    use rand::seq::SliceRandom;

    while let Some(data) = upload_rx.recv().await {
        // Coalesce: try_recv first, only sleep if nothing immediately available
        let mut payload = data.to_vec();
        match upload_rx.try_recv() {
            Ok(more) => {
                payload.extend_from_slice(&more);
                // Got data immediately — drain any more without waiting
                while payload.len() < max_payload {
                    match upload_rx.try_recv() {
                        Ok(more) => payload.extend_from_slice(&more),
                        Err(_) => break,
                    }
                }
            }
            Err(_) => {
                // Nothing available — brief wait for batching, then drain
                tokio::time::sleep(Duration::from_millis(2)).await;
                while payload.len() < max_payload {
                    match upload_rx.try_recv() {
                        Ok(more) => payload.extend_from_slice(&more),
                        Err(_) => break,
                    }
                }
            }
        }

        let seq = upload_seq.fetch_add(1, Ordering::Relaxed);
        let body = encode_request(seq, &payload, encoding);

        let path = data_paths
            .choose(&mut rand::thread_rng())
            .cloned()
            .unwrap_or_else(|| "/api/v1/data".to_string());

        let _permit = upload_sem.acquire().await;

        let mut req_builder = http::Request::builder()
            .method("POST")
            .uri(&path)
            .header("content-type", encoding.content_type())
            .header("cookie", &cookie_header)
            .header("host", &host)
            .header("cache-control", "no-cache, no-store");

        if let Some(ref ua) = user_agent {
            req_builder = req_builder.header("user-agent", ua.as_str());
        }
        for (k, v) in &extra_headers {
            req_builder = req_builder.header(k.as_str(), v.as_str());
        }

        let req = match req_builder.body(http_body_util::Full::new(Bytes::from(body))) {
            Ok(r) => r,
            Err(e) => {
                warn!("XPorta upload request build error: {}", e);
                continue;
            }
        };

        match sender.send_request(req).await {
            Ok(response) => {
                // Read response for piggyback download data
                if let Ok(body_bytes) = http_body_util::BodyExt::collect(response.into_body()).await
                {
                    let resp_data = body_bytes.to_bytes();
                    if let Some((dl_seq, dl_data)) = decode_response(&resp_data, encoding) {
                        if let (Some(seq), Some(data)) = (dl_seq, dl_data) {
                            let mut reassembler = dl_reassembler.lock().await;
                            let _ = reassembler.insert(seq, data);
                            for chunk in reassembler.drain() {
                                if download_tx.send(Bytes::from(chunk)).await.is_err() {
                                    return;
                                }
                            }
                        }
                    }
                }
            }
            Err(e) => {
                warn!("XPorta upload error: {}", e);
                return;
            }
        }
    }
}

/// Poll loop: maintains a pending GET request, waits for data or timeout, immediately sends replacement.
#[allow(clippy::too_many_arguments)]
async fn poll_loop(
    mut sender: hyper::client::conn::http2::SendRequest<http_body_util::Full<Bytes>>,
    poll_paths: Vec<String>,
    cookie_header: String,
    host: String,
    extra_headers: Vec<(String, String)>,
    user_agent: Option<String>,
    dl_reassembler: Arc<Mutex<Reassembler>>,
    download_tx: mpsc::Sender<Bytes>,
) {
    use rand::seq::SliceRandom;

    let mut last_dl_seq: u32 = 0;

    loop {
        let path = poll_paths
            .choose(&mut rand::thread_rng())
            .cloned()
            .unwrap_or_else(|| "/api/v1/notifications".to_string());

        let cache_buster: u64 = rand::random();
        let url = format!("{}?since={}&_t={}", path, last_dl_seq, cache_buster);

        let mut req_builder = http::Request::builder()
            .method("GET")
            .uri(&url)
            .header("cookie", &cookie_header)
            .header("host", &host)
            .header("cache-control", "no-cache, no-store");

        if let Some(ref ua) = user_agent {
            req_builder = req_builder.header("user-agent", ua.as_str());
        }
        for (k, v) in &extra_headers {
            req_builder = req_builder.header(k.as_str(), v.as_str());
        }

        let req = match req_builder.body(http_body_util::Full::new(Bytes::new())) {
            Ok(r) => r,
            Err(e) => {
                warn!("XPorta poll request build error: {}", e);
                return;
            }
        };

        match sender.send_request(req).await {
            Ok(response) => {
                if let Ok(body_bytes) = http_body_util::BodyExt::collect(response.into_body()).await
                {
                    let resp_data = body_bytes.to_bytes();
                    if let Some(items) = decode_poll_response(&resp_data) {
                        let mut reassembler = dl_reassembler.lock().await;
                        for (seq, data) in items {
                            if seq >= last_dl_seq {
                                last_dl_seq = seq + 1;
                            }
                            let _ = reassembler.insert(seq, data);
                        }
                        for chunk in reassembler.drain() {
                            if download_tx.send(Bytes::from(chunk)).await.is_err() {
                                return;
                            }
                        }
                    }
                }
            }
            Err(e) => {
                warn!("XPorta poll error: {}", e);
                // Brief delay before retrying
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            }
        }
    }
}
