use std::sync::atomic::Ordering;
use std::sync::Arc;

use anyhow::Result;
use quinn::Endpoint;
use tokio::sync::Semaphore;
use tracing::{debug, info, warn};

use prisma_core::cache::DnsCache;
use prisma_core::config::server::ServerConfig;
use prisma_core::port_hop;

use crate::auth::AuthStore;
use crate::handler;
use crate::state::ServerContext;

pub async fn listen(
    config: &ServerConfig,
    auth: AuthStore,
    dns_cache: DnsCache,
    ctx: ServerContext,
) -> Result<()> {
    let tls_config = build_tls_config(config)?;
    let mut server_config = quinn::ServerConfig::with_crypto(Arc::new(
        quinn::crypto::rustls::QuicServerConfig::try_from(tls_config)?,
    ));

    // Apply congestion control configuration
    let cc_mode = prisma_core::congestion::CongestionMode::from_config(
        &config.congestion.mode,
        config.congestion.target_bandwidth.as_deref(),
    );
    let mut transport_config = quinn::TransportConfig::default();
    transport_config.congestion_controller_factory(cc_mode.build_factory());
    server_config.transport_config(Arc::new(transport_config));

    info!(mode = %config.congestion.mode, "Server QUIC congestion control configured");

    let max_conn = config.performance.max_connections as usize;
    let semaphore = Arc::new(Semaphore::new(max_conn));

    let salamander_password = config.camouflage.salamander_password.clone();

    if config.port_hopping.enabled {
        return listen_with_port_hopping(
            config,
            server_config,
            auth,
            dns_cache,
            ctx,
            semaphore,
            salamander_password,
        )
        .await;
    }

    let endpoint = create_server_endpoint(
        server_config,
        config.quic_listen_addr.parse()?,
        salamander_password.as_deref(),
    )?;
    info!(addr = %config.quic_listen_addr, "QUIC listener started");

    // Use H3 masquerade accept loop when configured.
    if h3_masquerade_enabled(config) {
        info!("H3 masquerade enabled on QUIC listener");
        super::h3_masquerade::accept_loop(
            endpoint,
            Arc::new(config.clone()),
            auth,
            dns_cache,
            ctx,
            semaphore,
        )
        .await;
        return Ok(());
    }

    accept_loop(
        endpoint,
        auth,
        dns_cache,
        config.port_forwarding.clone(),
        ctx,
        semaphore,
    )
    .await;

    Ok(())
}

/// Create a quinn Endpoint, optionally wrapping the socket with Salamander obfuscation.
fn create_server_endpoint(
    server_config: quinn::ServerConfig,
    addr: std::net::SocketAddr,
    salamander_password: Option<&str>,
) -> Result<Endpoint> {
    let runtime =
        quinn::default_runtime().ok_or_else(|| anyhow::anyhow!("no async runtime found"))?;
    let socket = std::net::UdpSocket::bind(addr)?;
    let udp_socket = runtime.wrap_udp_socket(socket)?;

    let socket: Arc<dyn quinn::AsyncUdpSocket> = if let Some(password) = salamander_password {
        info!("Salamander UDP obfuscation enabled");
        prisma_core::salamander::SalamanderSocket::wrap(udp_socket, password.as_bytes())
    } else {
        udp_socket
    };

    // Support both QUIC v1 and v2 (RFC 9369) so clients can use either.
    let mut endpoint_config = quinn::EndpointConfig::default();
    endpoint_config.supported_versions(vec![1, prisma_core::types::QUIC_VERSION_2]);

    let endpoint =
        Endpoint::new_with_abstract_socket(endpoint_config, Some(server_config), socket, runtime)?;
    Ok(endpoint)
}

/// Port hopping mode: periodically rotate endpoints across ports.
async fn listen_with_port_hopping(
    config: &ServerConfig,
    server_config: quinn::ServerConfig,
    auth: AuthStore,
    dns_cache: DnsCache,
    ctx: ServerContext,
    semaphore: Arc<Semaphore>,
    salamander_password: Option<String>,
) -> Result<()> {
    use std::collections::HashMap;
    use std::time::SystemTime;

    let hop_config = &config.port_hopping;

    // Get shared secret from first authorized client (same as port computation)
    let secret = {
        let auth_store = ctx.state.auth_store.read().await;
        auth_store
            .clients
            .values()
            .next()
            .map(|e| e.auth_secret)
            .unwrap_or([0u8; 32])
    };

    let host = config
        .quic_listen_addr
        .split(':')
        .next()
        .unwrap_or("0.0.0.0");

    let mut active_endpoints: HashMap<u16, Endpoint> = HashMap::new();

    loop {
        let now = SystemTime::now();
        let ports = port_hop::active_ports(hop_config, &secret, now);
        let next_hop_secs = port_hop::seconds_until_next_hop(hop_config, now);

        // Start endpoints for new ports
        for &port in &ports {
            if let std::collections::hash_map::Entry::Vacant(entry) = active_endpoints.entry(port) {
                let addr_str = format!("{}:{}", host, port);
                match addr_str.parse() {
                    Ok(addr) => match create_server_endpoint(
                        server_config.clone(),
                        addr,
                        salamander_password.as_deref(),
                    ) {
                        Ok(endpoint) => {
                            info!(port, "Port hopping: activated port");

                            // Spawn acceptor for this endpoint
                            let ep = endpoint.clone();
                            let auth = auth.clone();
                            let dns = dns_cache.clone();
                            let fwd = config.port_forwarding.clone();
                            let ctx = ctx.clone();
                            let sem = semaphore.clone();

                            tokio::spawn(async move {
                                accept_loop(ep, auth, dns, fwd, ctx, sem).await;
                            });

                            entry.insert(endpoint);
                        }
                        Err(e) => {
                            warn!(port, error = %e, "Failed to bind port hopping endpoint");
                        }
                    },
                    Err(e) => {
                        warn!(port, error = %e, "Invalid port hopping address");
                    }
                }
            }
        }

        // Remove endpoints for ports no longer active
        let stale_ports: Vec<u16> = active_endpoints
            .keys()
            .filter(|p| !ports.contains(p))
            .copied()
            .collect();
        for port in stale_ports {
            if let Some(endpoint) = active_endpoints.remove(&port) {
                debug!(port, "Port hopping: deactivated port");
                endpoint.close(0u32.into(), b"port-hop");
            }
        }

        // Sleep until the next hop or check interval (whichever is shorter)
        let sleep_secs = next_hop_secs.clamp(1, 5);
        tokio::time::sleep(std::time::Duration::from_secs(sleep_secs)).await;
    }
}

/// Accept loop for a single quinn endpoint.
async fn accept_loop(
    endpoint: Endpoint,
    auth: AuthStore,
    dns_cache: DnsCache,
    fwd: prisma_core::config::server::PortForwardingConfig,
    ctx: ServerContext,
    semaphore: Arc<Semaphore>,
) {
    while let Some(incoming) = endpoint.accept().await {
        let auth = auth.clone();
        let dns = dns_cache.clone();
        let fwd = fwd.clone();
        let ctx = ctx.clone();
        let semaphore = semaphore.clone();
        tokio::spawn(async move {
            match incoming.await {
                Ok(connection) => {
                    let remote = connection.remote_address();
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
                                        warn!(error = %e, "QUIC stream handler error");
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
}

/// Check whether H3 masquerade is configured.
fn h3_masquerade_enabled(config: &ServerConfig) -> bool {
    config.camouflage.enabled
        && (config.camouflage.h3_cover_site.is_some() || config.camouflage.h3_static_dir.is_some())
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
        // Custom ALPN from config
        tls_config.alpn_protocols = config
            .camouflage
            .alpn_protocols
            .iter()
            .map(|s| s.as_bytes().to_vec())
            .collect();
    } else {
        // Standard ALPN — "h3" only, no legacy "prisma-v3"/"prisma-v2"
        tls_config.alpn_protocols = vec![prisma_core::types::PRISMA_QUIC_ALPN.as_bytes().to_vec()];
    }

    // When H3 masquerade is enabled, ensure the h3 ALPN is present
    // for browser/prober compatibility.
    if h3_masquerade_enabled(config) {
        let h3_alpn = b"h3".to_vec();
        if !tls_config.alpn_protocols.contains(&h3_alpn) {
            tls_config.alpn_protocols.push(h3_alpn);
        }
    }

    Ok(tls_config)
}
