use anyhow::Result;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tracing::debug;

/// A transport connection to the remote Prisma server.
/// Wraps TCP, QUIC, or TLS-on-TCP into a unified AsyncRead + AsyncWrite.
#[allow(clippy::large_enum_variant)]
pub enum TransportStream {
    Tcp(TcpStream),
    Quic(QuicBiStream),
    TcpTls(tokio_rustls::client::TlsStream<TcpStream>),
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
) -> Result<TransportStream> {
    debug!(addr = %server_addr, "Connecting to server via QUIC");

    let tls_config = build_client_tls_config(skip_cert_verify, alpn_protocols);

    let client_config = quinn::ClientConfig::new(Arc::new(
        quinn::crypto::rustls::QuicClientConfig::try_from(tls_config)?,
    ));

    let mut endpoint = quinn::Endpoint::client("0.0.0.0:0".parse()?)?;
    endpoint.set_default_client_config(client_config);

    let addr = server_addr.parse()?;
    let connection = endpoint.connect(addr, server_name)?.await?;
    let (send, recv) = connection.open_bi().await?;

    Ok(TransportStream::Quic(QuicBiStream { send, recv }))
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
