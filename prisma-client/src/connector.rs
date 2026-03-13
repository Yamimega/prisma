use anyhow::Result;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tracing::debug;

use crate::grpc_stream::GrpcStream;
use crate::ws_stream::WsStream;
use crate::xhttp_stream::XhttpStream;

/// A transport connection to the remote Prisma server.
/// Wraps TCP, QUIC, or TLS-on-TCP into a unified AsyncRead + AsyncWrite.
#[allow(clippy::large_enum_variant)]
pub enum TransportStream {
    Tcp(TcpStream),
    Quic(QuicBiStream),
    TcpTls(tokio_rustls::client::TlsStream<TcpStream>),
    WebSocket(WsStream),
    Grpc(GrpcStream),
    Xhttp(XhttpStream),
}

pub struct QuicBiStream {
    pub send: quinn::SendStream,
    pub recv: quinn::RecvStream,
}

impl AsyncRead for TransportStream {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            TransportStream::Tcp(s) => std::pin::Pin::new(s).poll_read(cx, buf),
            TransportStream::Quic(s) => std::pin::Pin::new(&mut s.recv).poll_read(cx, buf),
            TransportStream::TcpTls(s) => std::pin::Pin::new(s).poll_read(cx, buf),
            TransportStream::WebSocket(s) => std::pin::Pin::new(s).poll_read(cx, buf),
            TransportStream::Grpc(s) => std::pin::Pin::new(s).poll_read(cx, buf),
            TransportStream::Xhttp(s) => std::pin::Pin::new(s).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for TransportStream {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        match self.get_mut() {
            TransportStream::Tcp(s) => std::pin::Pin::new(s).poll_write(cx, buf),
            TransportStream::Quic(s) => match std::pin::Pin::new(&mut s.send).poll_write(cx, buf) {
                std::task::Poll::Ready(Ok(n)) => std::task::Poll::Ready(Ok(n)),
                std::task::Poll::Ready(Err(e)) => {
                    std::task::Poll::Ready(Err(std::io::Error::other(e)))
                }
                std::task::Poll::Pending => std::task::Poll::Pending,
            },
            TransportStream::TcpTls(s) => std::pin::Pin::new(s).poll_write(cx, buf),
            TransportStream::WebSocket(s) => std::pin::Pin::new(s).poll_write(cx, buf),
            TransportStream::Grpc(s) => std::pin::Pin::new(s).poll_write(cx, buf),
            TransportStream::Xhttp(s) => std::pin::Pin::new(s).poll_write(cx, buf),
        }
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            TransportStream::Tcp(s) => std::pin::Pin::new(s).poll_flush(cx),
            TransportStream::Quic(s) => match std::pin::Pin::new(&mut s.send).poll_flush(cx) {
                std::task::Poll::Ready(Ok(())) => std::task::Poll::Ready(Ok(())),
                std::task::Poll::Ready(Err(e)) => {
                    std::task::Poll::Ready(Err(std::io::Error::other(e)))
                }
                std::task::Poll::Pending => std::task::Poll::Pending,
            },
            TransportStream::TcpTls(s) => std::pin::Pin::new(s).poll_flush(cx),
            TransportStream::WebSocket(s) => std::pin::Pin::new(s).poll_flush(cx),
            TransportStream::Grpc(s) => std::pin::Pin::new(s).poll_flush(cx),
            TransportStream::Xhttp(s) => std::pin::Pin::new(s).poll_flush(cx),
        }
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            TransportStream::Tcp(s) => std::pin::Pin::new(s).poll_shutdown(cx),
            TransportStream::Quic(s) => match std::pin::Pin::new(&mut s.send).poll_shutdown(cx) {
                std::task::Poll::Ready(Ok(())) => std::task::Poll::Ready(Ok(())),
                std::task::Poll::Ready(Err(e)) => {
                    std::task::Poll::Ready(Err(std::io::Error::other(e)))
                }
                std::task::Poll::Pending => std::task::Poll::Pending,
            },
            TransportStream::TcpTls(s) => std::pin::Pin::new(s).poll_shutdown(cx),
            TransportStream::WebSocket(s) => std::pin::Pin::new(s).poll_shutdown(cx),
            TransportStream::Grpc(s) => std::pin::Pin::new(s).poll_shutdown(cx),
            TransportStream::Xhttp(s) => std::pin::Pin::new(s).poll_shutdown(cx),
        }
    }
}

/// Connect to the remote Prisma server via TCP.
pub async fn connect_tcp(server_addr: &str) -> Result<TransportStream> {
    debug!(addr = %server_addr, "Connecting to server via TCP");
    let stream = TcpStream::connect(server_addr).await?;
    Ok(TransportStream::Tcp(stream))
}

/// Connect to the remote Prisma server via TCP wrapped in TLS.
pub async fn connect_tcp_tls(
    server_addr: &str,
    server_name: &str,
    skip_cert_verify: bool,
    alpn_protocols: &[String],
) -> Result<TransportStream> {
    debug!(addr = %server_addr, sni = %server_name, "Connecting to server via TLS-on-TCP");

    let tls_config = build_client_tls_config(skip_cert_verify, alpn_protocols);

    let connector = tokio_rustls::TlsConnector::from(Arc::new(tls_config));
    let tcp_stream = TcpStream::connect(server_addr).await?;
    let sni = rustls::pki_types::ServerName::try_from(server_name.to_string())?;
    let tls_stream = connector.connect(sni, tcp_stream).await?;

    Ok(TransportStream::TcpTls(tls_stream))
}

/// Connect to the remote Prisma server via QUIC.
pub async fn connect_quic(
    server_addr: &str,
    skip_cert_verify: bool,
    alpn_protocols: &[String],
    server_name: &str,
    congestion_mode: &prisma_core::congestion::CongestionMode,
    salamander_password: Option<&str>,
) -> Result<TransportStream> {
    debug!(addr = %server_addr, "Connecting to server via QUIC");

    let tls_config = build_client_tls_config(skip_cert_verify, alpn_protocols);

    let mut client_config = quinn::ClientConfig::new(Arc::new(
        quinn::crypto::rustls::QuicClientConfig::try_from(tls_config)?,
    ));

    // Apply congestion control configuration
    let mut transport_config = quinn::TransportConfig::default();
    transport_config.congestion_controller_factory(congestion_mode.build_factory());
    client_config.transport_config(Arc::new(transport_config));

    let bind_addr: std::net::SocketAddr = "0.0.0.0:0".parse()?;
    let runtime =
        quinn::default_runtime().ok_or_else(|| anyhow::anyhow!("no async runtime found"))?;

    let socket = std::net::UdpSocket::bind(bind_addr)?;
    let udp_socket = runtime.wrap_udp_socket(socket)?;

    // Optionally wrap with Salamander obfuscation
    let socket: Arc<dyn quinn::AsyncUdpSocket> = if let Some(password) = salamander_password {
        debug!("Salamander UDP obfuscation enabled");
        prisma_core::salamander::SalamanderSocket::wrap(udp_socket, password.as_bytes())
    } else {
        udp_socket
    };

    let mut endpoint = quinn::Endpoint::new_with_abstract_socket(
        quinn::EndpointConfig::default(),
        None,
        socket,
        runtime,
    )?;
    endpoint.set_default_client_config(client_config);

    let addr = server_addr.parse()?;
    let connection = endpoint.connect(addr, server_name)?.await?;
    let (send, recv) = connection.open_bi().await?;

    Ok(TransportStream::Quic(QuicBiStream { send, recv }))
}

/// Connect to the remote Prisma server via WebSocket.
pub async fn connect_ws(
    ws_url: &str,
    skip_cert_verify: bool,
    extra_headers: &[(String, String)],
) -> Result<TransportStream> {
    debug!(url = %ws_url, "Connecting to server via WebSocket");

    let request = ws_url
        .parse::<http::Uri>()
        .map_err(|e| anyhow::anyhow!("Invalid WebSocket URL: {}", e))?;

    // Build the tungstenite request with extra headers
    let mut req_builder = http::Request::builder()
        .uri(ws_url)
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header(
            "Sec-WebSocket-Key",
            tokio_tungstenite::tungstenite::handshake::client::generate_key(),
        )
        .header("Sec-WebSocket-Version", "13");

    // Add Host header
    if let Some(host) = request.host() {
        let host_val = if let Some(port) = request.port() {
            format!("{}:{}", host, port)
        } else {
            host.to_string()
        };
        req_builder = req_builder.header("Host", host_val);
    }

    for (k, v) in extra_headers {
        req_builder = req_builder.header(k.as_str(), v.as_str());
    }

    let ws_request = req_builder
        .body(())
        .map_err(|e| anyhow::anyhow!("Failed to build WS request: {}", e))?;

    // Determine if wss:// (TLS) or ws://
    let is_tls = ws_url.starts_with("wss://");

    let host = request.host().unwrap_or("localhost");
    let port = request.port_u16().unwrap_or(if is_tls { 443 } else { 80 });
    let addr = format!("{}:{}", host, port);

    let tcp_stream = TcpStream::connect(&addr).await?;

    if is_tls {
        let tls_config = build_client_tls_config(skip_cert_verify, &[]);
        let connector = tokio_rustls::TlsConnector::from(Arc::new(tls_config));
        let sni = rustls::pki_types::ServerName::try_from(host.to_string())?;
        let tls_stream = connector.connect(sni, tcp_stream).await?;

        let (ws_stream, _response) = tokio_tungstenite::client_async(ws_request, tls_stream)
            .await
            .map_err(|e| anyhow::anyhow!("WebSocket handshake failed: {}", e))?;

        Ok(TransportStream::WebSocket(WsStream::new(ws_stream)))
    } else {
        let (ws_stream, _response) = tokio_tungstenite::client_async(ws_request, tcp_stream)
            .await
            .map_err(|e| anyhow::anyhow!("WebSocket handshake failed: {}", e))?;

        Ok(TransportStream::WebSocket(WsStream::new(ws_stream)))
    }
}

/// Connect to the remote Prisma server via gRPC.
pub async fn connect_grpc(grpc_url: &str) -> Result<TransportStream> {
    debug!(url = %grpc_url, "Connecting to server via gRPC");

    use prisma_core::proto::tunnel::prisma_tunnel_client::PrismaTunnelClient;
    use prisma_core::proto::tunnel::TunnelData;

    let channel = tonic::transport::Channel::from_shared(grpc_url.to_string())
        .map_err(|e| anyhow::anyhow!("Invalid gRPC URL: {}", e))?
        .connect()
        .await
        .map_err(|e| anyhow::anyhow!("gRPC connect failed: {}", e))?;

    let mut client = PrismaTunnelClient::new(channel);

    // Create a channel for outbound messages
    let (outbound_tx, outbound_rx) = mpsc::channel::<TunnelData>(64);
    let outbound_stream = tokio_stream::wrappers::ReceiverStream::new(outbound_rx);

    let response = client
        .tunnel(outbound_stream)
        .await
        .map_err(|e| anyhow::anyhow!("gRPC tunnel call failed: {}", e))?;

    let inbound = response.into_inner();
    let grpc_stream = GrpcStream::new(inbound, outbound_tx);

    Ok(TransportStream::Grpc(grpc_stream))
}

/// Connect to the remote Prisma server via XHTTP (stream-one mode).
/// Uses a streaming HTTP/2 POST for bidirectional communication.
pub async fn connect_xhttp(
    stream_url: &str,
    extra_headers: &[(String, String)],
    user_agent: Option<&str>,
    referer: Option<&str>,
) -> Result<TransportStream> {
    debug!(url = %stream_url, "Connecting to server via XHTTP stream-one");

    let uri: http::Uri = stream_url
        .parse()
        .map_err(|e| anyhow::anyhow!("Invalid XHTTP URL: {}", e))?;

    let host = uri.host().unwrap_or("localhost");
    let port = uri.port_u16().unwrap_or(443);
    let is_tls = uri.scheme_str() == Some("https");
    let addr = format!("{}:{}", host, port);

    let (upload_tx, upload_rx) = mpsc::channel::<bytes::Bytes>(256);
    let (download_tx, download_rx) = mpsc::channel::<bytes::Bytes>(256);

    // Build the HTTP request
    let mut req_builder = http::Request::builder()
        .method("POST")
        .uri(stream_url)
        .header("content-type", "application/octet-stream")
        .header("transfer-encoding", "chunked");

    // Add obfuscation headers
    let ua = user_agent.unwrap_or("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36");
    req_builder = req_builder.header("user-agent", ua);
    if let Some(ref_val) = referer {
        req_builder = req_builder.header("referer", ref_val);
    }
    req_builder = req_builder.header("accept-language", "en-US,en;q=0.9");

    for (k, v) in extra_headers {
        req_builder = req_builder.header(k.as_str(), v.as_str());
    }

    // Spawn a task to drive the HTTP connection
    let tcp_stream = TcpStream::connect(&addr).await?;

    if is_tls {
        let tls_config = build_client_tls_config(true, &["h2".to_string()]);
        let connector = tokio_rustls::TlsConnector::from(Arc::new(tls_config));
        let sni = rustls::pki_types::ServerName::try_from(host.to_string())?;
        let tls_stream = connector.connect(sni, tcp_stream).await?;

        // Use hyper for HTTP/2 streaming
        spawn_xhttp_client(tls_stream, req_builder, upload_rx, download_tx).await?;
    } else {
        spawn_xhttp_client(tcp_stream, req_builder, upload_rx, download_tx).await?;
    }

    Ok(TransportStream::Xhttp(XhttpStream::new(
        download_rx,
        upload_tx,
    )))
}

async fn spawn_xhttp_client<S>(
    stream: S,
    req_builder: http::request::Builder,
    mut upload_rx: mpsc::Receiver<bytes::Bytes>,
    download_tx: mpsc::Sender<bytes::Bytes>,
) -> Result<()>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    use http_body_util::StreamBody;
    use hyper::body::Frame;
    use hyper_util::rt::TokioIo;

    let io = TokioIo::new(stream);
    let (mut sender, conn) =
        hyper::client::conn::http2::handshake(hyper_util::rt::TokioExecutor::new(), io)
            .await
            .map_err(|e| anyhow::anyhow!("H2 handshake failed: {}", e))?;

    // Drive the connection in background
    tokio::spawn(async move {
        if let Err(e) = conn.await {
            tracing::warn!("XHTTP H2 connection error: {}", e);
        }
    });

    // Create a streaming body from upload channel
    let (body_tx, body_rx) =
        mpsc::channel::<Result<Frame<bytes::Bytes>, std::convert::Infallible>>(64);
    let body = StreamBody::new(tokio_stream::wrappers::ReceiverStream::new(body_rx));

    let req = req_builder
        .body(body)
        .map_err(|e| anyhow::anyhow!("Failed to build request: {}", e))?;

    // Send request and get response
    let response_fut = sender.send_request(req);

    // Spawn upload feeder
    tokio::spawn(async move {
        while let Some(data) = upload_rx.recv().await {
            if body_tx.send(Ok(Frame::data(data))).await.is_err() {
                break;
            }
        }
    });

    // Spawn download reader
    tokio::spawn(async move {
        match response_fut.await {
            Ok(response) => {
                use http_body_util::BodyExt;
                let mut body = response.into_body();
                while let Some(frame) = body.frame().await {
                    match frame {
                        Ok(f) => {
                            if let Some(data) = f.data_ref() {
                                if download_tx.send(data.clone()).await.is_err() {
                                    break;
                                }
                            }
                        }
                        Err(_) => break,
                    }
                }
            }
            Err(e) => {
                tracing::warn!("XHTTP response error: {}", e);
            }
        }
    });

    Ok(())
}

/// Build a `rustls::ClientConfig` with optional cert verification and ALPN.
fn build_client_tls_config(
    skip_cert_verify: bool,
    alpn_protocols: &[String],
) -> rustls::ClientConfig {
    let mut config = if skip_cert_verify {
        rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(InsecureCertVerifier))
            .with_no_client_auth()
    } else {
        let mut roots = rustls::RootCertStore::empty();
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth()
    };
    config.alpn_protocols = alpn_protocols
        .iter()
        .map(|s| s.as_bytes().to_vec())
        .collect();
    config
}

/// Certificate verifier that accepts any certificate (dev mode only).
#[derive(Debug)]
struct InsecureCertVerifier;

impl rustls::client::danger::ServerCertVerifier for InsecureCertVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
            rustls::SignatureScheme::ED25519,
            rustls::SignatureScheme::RSA_PSS_SHA256,
            rustls::SignatureScheme::RSA_PSS_SHA384,
            rustls::SignatureScheme::RSA_PSS_SHA512,
            rustls::SignatureScheme::RSA_PKCS1_SHA256,
            rustls::SignatureScheme::RSA_PKCS1_SHA384,
            rustls::SignatureScheme::RSA_PKCS1_SHA512,
        ]
    }
}
