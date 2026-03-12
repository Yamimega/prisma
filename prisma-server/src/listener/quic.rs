use std::sync::atomic::Ordering;
use std::sync::Arc;

use anyhow::Result;
use quinn::Endpoint;
use tokio::sync::Semaphore;
use tracing::{info, warn};

use prisma_core::cache::DnsCache;
use prisma_core::config::server::ServerConfig;

use prisma_core::state::ServerState;

use crate::auth::AuthStore;
use crate::handler;

pub async fn listen(
    config: &ServerConfig,
    auth: AuthStore,
    dns_cache: DnsCache,
    state: ServerState,
) -> Result<()> {
    let tls_config = build_tls_config(config)?;
    let server_config = quinn::ServerConfig::with_crypto(Arc::new(
        quinn::crypto::rustls::QuicServerConfig::try_from(tls_config)?,
    ));

    let max_conn = config.performance.max_connections as usize;
    let semaphore = Arc::new(Semaphore::new(max_conn));

    let endpoint = Endpoint::server(server_config, config.quic_listen_addr.parse()?)?;
    info!(addr = %config.quic_listen_addr, "QUIC listener started");

    while let Some(incoming) = endpoint.accept().await {
        let auth = auth.clone();
        let dns = dns_cache.clone();
        let fwd = config.port_forwarding.clone();
        let state = state.clone();
        let semaphore = semaphore.clone();
        tokio::spawn(async move {
            match incoming.await {
                Ok(connection) => {
                    let remote = connection.remote_address();
                    info!(peer = %remote, "New QUIC connection");

                    loop {
                        match connection.accept_bi().await {
                            Ok((send, recv)) => {
                                let permit = match semaphore.clone().try_acquire_owned() {
                                    Ok(p) => p,
                                    Err(_) => {
                                        warn!(peer = %remote, "QUIC stream rejected: max connections");
                                        continue;
                                    }
                                };
                                let auth = auth.clone();
                                let dns = dns.clone();
                                let fwd = fwd.clone();
                                let state = state.clone();
                                let peer_str = remote.to_string();
                                tokio::spawn(async move {
                                    state
                                        .metrics
                                        .total_connections
                                        .fetch_add(1, Ordering::Relaxed);
                                    state
                                        .metrics
                                        .active_connections
                                        .fetch_add(1, Ordering::Relaxed);
                                    if let Err(e) = handler::handle_quic_stream(
                                        send,
                                        recv,
                                        auth,
                                        dns,
                                        fwd,
                                        state.clone(),
                                        peer_str,
                                    )
                                    .await
                                    {
                                        warn!(error = %e, "QUIC stream handler error");
                                    }
                                    state
                                        .metrics
                                        .active_connections
                                        .fetch_sub(1, Ordering::Relaxed);
                                    drop(permit);
                                });
                            }
                            Err(quinn::ConnectionError::ApplicationClosed(_)) => break,
                            Err(e) => {
                                warn!(error = %e, "Failed to accept QUIC stream");
                                break;
                            }
                        }
                    }
                }
                Err(e) => {
                    warn!(error = %e, "Failed to accept QUIC connection");
                }
            }
        });
    }

    Ok(())
}

fn build_tls_config(config: &ServerConfig) -> Result<rustls::ServerConfig> {
    let tls = config
        .tls
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("TLS configuration required for QUIC"))?;

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

    if config.camouflage.enabled && !config.camouflage.alpn_protocols.is_empty() {
        tls_config.alpn_protocols = config
            .camouflage
            .alpn_protocols
            .iter()
            .map(|s| s.as_bytes().to_vec())
            .collect();
    } else {
        tls_config.alpn_protocols = vec![b"prisma-v1".to_vec()];
    }

    Ok(tls_config)
}
