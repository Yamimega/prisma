use std::sync::Arc;

use anyhow::Result;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tracing::{debug, warn};

use prisma_core::config::server::ServerConfig;
use prisma_core::types::PRISMA_PROTOCOL_VERSION;

/// Minimum ClientHello payload size in bytes.
const MIN_CLIENT_HELLO_SIZE: u16 = 41;

/// Check if the first 3 bytes of a connection look like a PrismaVeil ClientHello frame.
///
/// Wire format: `[len_hi:1][len_lo:1][version:1]...`
/// - `frame_len = u16::from_be_bytes([b[0], b[1]])` must be >= `MIN_CLIENT_HELLO_SIZE` (41)
/// - `version` must match `PRISMA_PROTOCOL_VERSION` (v4)
///
/// This rejects HTTP probes (`GET ` → version=0x54), TLS ClientHello (`0x16 0x03` → version varies),
/// and random bytes (version unlikely to match with valid length).
pub fn looks_like_prisma_hello(bytes: &[u8]) -> bool {
    if bytes.len() < 3 {
        return false;
    }
    // Reject TLS record layer: first byte 0x14-0x18 (ChangeCipherSpec, Alert, Handshake, Application)
    // with second byte 0x03 (TLS major version) is a TLS record, not PrismaVeil.
    if bytes[0] >= 0x14 && bytes[0] <= 0x18 && bytes[1] == 0x03 {
        return false;
    }
    let frame_len = u16::from_be_bytes([bytes[0], bytes[1]]);
    let version = bytes[2];
    frame_len >= MIN_CLIENT_HELLO_SIZE && version == PRISMA_PROTOCOL_VERSION
}

/// Build a `tokio_rustls::TlsAcceptor` for wrapping TCP connections in TLS.
/// Reuses the same cert/key files as QUIC.
pub fn build_tcp_tls_acceptor(config: &ServerConfig) -> Result<tokio_rustls::TlsAcceptor> {
    let tls = config
        .tls
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("TLS configuration required for tls_on_tcp"))?;

    let cert_pem = std::fs::read(&tls.cert_path)?;
    let key_pem = std::fs::read(&tls.key_path)?;

    let certs: Vec<rustls::pki_types::CertificateDer> =
        rustls_pemfile::certs(&mut cert_pem.as_slice())
            .filter_map(|r| r.ok())
            .collect();

    let key = rustls_pemfile::private_key(&mut key_pem.as_slice())?
        .ok_or_else(|| anyhow::anyhow!("No private key found in {}", tls.key_path))?;

    let mut tls_config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)?;

    tls_config.alpn_protocols = config
        .camouflage
        .alpn_protocols
        .iter()
        .map(|s| s.as_bytes().to_vec())
        .collect();

    Ok(tokio_rustls::TlsAcceptor::from(Arc::new(tls_config)))
}

/// Relay a non-Prisma connection to a decoy/fallback server.
///
/// Connects to `fallback_addr`, writes the `initial_bytes` that were already peeked,
/// then bidirectionally copies data between the client and the fallback server.
pub async fn decoy_relay<S>(mut stream: S, fallback_addr: &str, initial_bytes: &[u8]) -> Result<()>
where
    S: AsyncReadExt + AsyncWriteExt + Unpin,
{
    debug!(fallback = %fallback_addr, "Relaying to decoy server");

    let mut fallback = TcpStream::connect(fallback_addr).await.map_err(|e| {
        warn!(fallback = %fallback_addr, error = %e, "Failed to connect to decoy server");
        e
    })?;

    // Forward the bytes we already peeked
    fallback.write_all(initial_bytes).await?;

    // Bidirectional copy
    tokio::io::copy_bidirectional(&mut stream, &mut fallback).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prisma_hello_too_short_frame() {
        // frame_len=10 (too small), version=0x01
        assert!(!looks_like_prisma_hello(&[0x00, 0x0A, 0x01]));
    }

    #[test]
    fn test_prisma_hello_wrong_version() {
        // frame_len=100, version=0x03 (old v3, no longer accepted)
        assert!(!looks_like_prisma_hello(&[0x00, 0x64, 0x03]));
    }

    #[test]
    fn test_prisma_hello_http_get() {
        // "GET " → 0x47, 0x45, 0x54
        assert!(!looks_like_prisma_hello(&[0x47, 0x45, 0x54]));
    }

    #[test]
    fn test_prisma_hello_tls_client_hello() {
        // TLS record: 0x16, 0x03, 0x01
        assert!(!looks_like_prisma_hello(&[0x16, 0x03, 0x01]));
    }

    #[test]
    fn test_prisma_hello_valid() {
        // frame_len=100 (0x0064), version=0x04 (v4)
        assert!(looks_like_prisma_hello(&[0x00, 0x64, 0x04]));
    }

    #[test]
    fn test_prisma_hello_too_few_bytes() {
        assert!(!looks_like_prisma_hello(&[0x00, 0x64]));
        assert!(!looks_like_prisma_hello(&[]));
    }
}
