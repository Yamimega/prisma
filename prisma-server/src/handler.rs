use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use anyhow::Result;
use chrono::Utc;
use prisma_core::cache::DnsCache;
use prisma_core::config::server::{PortForwardingConfig, RuleAction, RuleCondition};
use prisma_core::crypto::aead::create_cipher;
use prisma_core::crypto::kdf::derive_ticket_key;
use prisma_core::protocol::codec::*;
use prisma_core::protocol::handshake::PrismaHandshakeServer;
use prisma_core::protocol::types::*;
use prisma_core::types::{PaddingRange, ProxyAddress, ProxyDestination, PRISMA_PROTOCOL_VERSION};
use prisma_core::util;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tracing::{info, warn};

use prisma_core::state::{ConnectionInfo, ServerState, SessionMode, Transport};

use crate::auth::AuthStore;
use crate::camouflage;
use crate::forward;
use crate::outbound;
use crate::relay;
use crate::state::ServerContext;
use crate::udp_relay;

/// Default server features bitmask.
const DEFAULT_SERVER_FEATURES: u32 = FEATURE_UDP_RELAY | FEATURE_SPEED_TEST | FEATURE_DNS_TUNNEL;

/// Handle an incoming TCP connection through the PrismaVeil protocol.
pub async fn handle_tcp_connection(
    mut stream: TcpStream,
    auth: AuthStore,
    dns_cache: DnsCache,
    forward_config: PortForwardingConfig,
    ctx: ServerContext,
    peer_addr: String,
) -> Result<()> {
    let padding_range = {
        let cfg = ctx.state.config.read().await;
        PaddingRange::new(cfg.padding.min, cfg.padding.max)
    };
    let session_keys = {
        let (mut read, mut write) = stream.split();
        match perform_handshake(&mut read, &mut write, &auth, padding_range, &ctx.state).await {
            Ok(keys) => keys,
            Err(e) => {
                ctx.state
                    .metrics
                    .handshake_failures
                    .fetch_add(1, Ordering::Relaxed);
                return Err(e);
            }
        }
    };
    info!(session_id = %session_keys.session_id, "Handshake complete (TCP, v4)");

    let (read, write) = stream.into_split();
    run_registered_session(
        session_keys,
        read,
        write,
        Transport::Tcp,
        peer_addr,
        &auth,
        dns_cache,
        forward_config,
        ctx,
    )
    .await
}

/// Handle an incoming TCP connection with camouflage: peek first 3 bytes to
/// decide whether this is a PrismaVeil client or a probe/browser that should
/// be relayed to the decoy fallback.
pub async fn handle_tcp_connection_camouflaged<S>(
    mut stream: S,
    auth: AuthStore,
    dns_cache: DnsCache,
    forward_config: PortForwardingConfig,
    ctx: ServerContext,
    peer_addr: String,
    fallback_addr: Option<String>,
) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let state = &ctx.state;
    // Peek first 3 bytes with a timeout
    let mut peek = [0u8; 3];
    match tokio::time::timeout(
        std::time::Duration::from_secs(10),
        stream.read_exact(&mut peek),
    )
    .await
    {
        Ok(Ok(_)) => {}
        Ok(Err(e)) => return Err(e.into()),
        Err(_) => {
            // Timeout reading peek bytes — treat as probe
            if let Some(ref fallback) = fallback_addr {
                let _ = camouflage::decoy_relay(stream, fallback, &[]).await;
            }
            return Ok(());
        }
    }

    if !camouflage::looks_like_prisma_hello(&peek) {
        // Not a Prisma client — relay to decoy
        if let Some(ref fallback) = fallback_addr {
            let _ = camouflage::decoy_relay(stream, fallback, &peek).await;
        }
        return Ok(());
    }

    // It looks like Prisma: read the rest of the ClientHello/ClientInit frame
    let frame_len = u16::from_be_bytes([peek[0], peek[1]]) as usize;
    let mut client_hello_buf = vec![0u8; frame_len];
    // The first byte after length prefix is peek[2] (version byte)
    client_hello_buf[0] = peek[2];
    if frame_len > 1 {
        stream.read_exact(&mut client_hello_buf[1..]).await?;
    }

    let padding_range = {
        let cfg = state.config.read().await;
        PaddingRange::new(cfg.padding.min, cfg.padding.max)
    };

    let version = client_hello_buf[0];

    if version != PRISMA_PROTOCOL_VERSION {
        // Not a supported protocol version — relay to fallback (treat as probe)
        warn!(
            version,
            "Unsupported protocol version, relaying to fallback"
        );
        if let Some(ref fallback) = fallback_addr {
            let mut frame_bytes = Vec::with_capacity(2 + frame_len);
            frame_bytes.extend_from_slice(&peek[0..2]);
            frame_bytes.extend_from_slice(&client_hello_buf);
            let _ = camouflage::decoy_relay(stream, fallback, &frame_bytes).await;
        }
        return Ok(());
    }

    // v4 handshake: 2-step
    let ticket_key = derive_ticket_key_from_state(state).await;

    let bucket_sizes = {
        let cfg = state.config.read().await;
        let ts = &cfg.traffic_shaping;
        let mode = prisma_core::traffic_shaping::PaddingMode::from_str(&ts.padding_mode);
        if mode == prisma_core::traffic_shaping::PaddingMode::Bucket {
            ts.bucket_sizes.clone()
        } else {
            Vec::new()
        }
    };

    let (server_init_bytes, session_keys) = match PrismaHandshakeServer::process_client_init(
        &client_hello_buf,
        padding_range,
        DEFAULT_SERVER_FEATURES,
        &ticket_key,
        &bucket_sizes,
        &auth,
    ) {
        Ok((bytes, state)) => (bytes, state.into_session_keys()),
        Err(e) => {
            warn!(error = %e, "ClientInit processing failed");
            state
                .metrics
                .handshake_failures
                .fetch_add(1, Ordering::Relaxed);
            if let Some(ref fallback) = fallback_addr {
                let mut frame_bytes = Vec::with_capacity(2 + frame_len);
                frame_bytes.extend_from_slice(&peek[0..2]);
                frame_bytes.extend_from_slice(&client_hello_buf);
                let _ = camouflage::decoy_relay(stream, fallback, &frame_bytes).await;
            }
            return Ok(());
        }
    };

    util::write_framed(&mut stream, &server_init_bytes).await?;

    info!(session_id = %session_keys.session_id, "Handshake complete (TCP camouflaged, v4)");

    let (read, write) = tokio::io::split(stream);
    run_registered_session(
        session_keys,
        read,
        write,
        Transport::Tcp,
        peer_addr,
        &auth,
        dns_cache,
        forward_config,
        ctx,
    )
    .await
}

/// Handle an incoming QUIC bidirectional stream.
pub async fn handle_quic_stream(
    mut send: quinn::SendStream,
    mut recv: quinn::RecvStream,
    auth: AuthStore,
    dns_cache: DnsCache,
    forward_config: PortForwardingConfig,
    ctx: ServerContext,
    peer_addr: String,
) -> Result<()> {
    let padding_range = {
        let cfg = ctx.state.config.read().await;
        PaddingRange::new(cfg.padding.min, cfg.padding.max)
    };
    let session_keys =
        match perform_handshake(&mut recv, &mut send, &auth, padding_range, &ctx.state).await {
            Ok(keys) => keys,
            Err(e) => {
                ctx.state
                    .metrics
                    .handshake_failures
                    .fetch_add(1, Ordering::Relaxed);
                return Err(e);
            }
        };

    info!(session_id = %session_keys.session_id, "Handshake complete (QUIC, v4)");
    run_registered_session(
        session_keys,
        recv,
        send,
        Transport::Quic,
        peer_addr,
        &auth,
        dns_cache,
        forward_config,
        ctx,
    )
    .await
}

/// Unified handshake over any AsyncRead + AsyncWrite pair (v4 only).
async fn perform_handshake<R, W>(
    reader: &mut R,
    writer: &mut W,
    auth: &AuthStore,
    padding_range: PaddingRange,
    state: &ServerState,
) -> Result<SessionKeys>
where
    R: AsyncReadExt + Unpin,
    W: AsyncWriteExt + Unpin,
{
    let client_init_buf = util::read_framed(reader).await?;

    if client_init_buf.is_empty() {
        return Err(anyhow::anyhow!("Empty client init"));
    }

    let version = client_init_buf[0];

    if version != PRISMA_PROTOCOL_VERSION {
        return Err(anyhow::anyhow!(
            "Unsupported protocol version: 0x{:02x}, expected v4 (0x{:02x})",
            version,
            PRISMA_PROTOCOL_VERSION
        ));
    }

    // v4 handshake: 2-step with bucket sizes
    let ticket_key = derive_ticket_key_from_state(state).await;
    let bucket_sizes = {
        let cfg = state.config.read().await;
        let ts = &cfg.traffic_shaping;
        let mode = prisma_core::traffic_shaping::PaddingMode::from_str(&ts.padding_mode);
        if mode == prisma_core::traffic_shaping::PaddingMode::Bucket {
            ts.bucket_sizes.clone()
        } else {
            Vec::new()
        }
    };
    let (server_init_bytes, server_state) = PrismaHandshakeServer::process_client_init(
        &client_init_buf,
        padding_range,
        DEFAULT_SERVER_FEATURES,
        &ticket_key,
        &bucket_sizes,
        auth,
    )?;

    util::write_framed(writer, &server_init_bytes).await?;

    Ok(server_state.into_session_keys())
}

/// Derive a ticket key from server state (uses first authorized client's secret as seed).
async fn derive_ticket_key_from_state(state: &ServerState) -> [u8; 32] {
    let auth_store = state.auth_store.read().await;
    // Use a deterministic seed from server config
    if let Some(entry) = auth_store.clients.values().next() {
        derive_ticket_key(&entry.auth_secret)
    } else {
        // Fallback: all zeros (should not happen in production)
        [0u8; 32]
    }
}

/// Register a session in state, verify challenge response, run it, then clean up on exit.
#[allow(clippy::too_many_arguments)]
async fn run_registered_session<R, W>(
    session_keys: SessionKeys,
    read: R,
    write: W,
    transport: Transport,
    peer_addr: String,
    auth: &AuthStore,
    dns_cache: DnsCache,
    forward_config: PortForwardingConfig,
    ctx: ServerContext,
) -> Result<()>
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let client_name = auth.client_name(&session_keys.client_id);
    let display_name = client_name.clone().unwrap_or_else(|| "unknown".into());
    info!(
        client_id = %session_keys.client_id.0,
        client_name = %display_name,
        peer = %peer_addr,
        transport = ?transport,
        protocol = "v4",
        "Client connected"
    );

    let bytes_up = Arc::new(AtomicU64::new(0));
    let bytes_down = Arc::new(AtomicU64::new(0));
    let conn_info = ConnectionInfo {
        session_id: session_keys.session_id,
        client_id: Some(session_keys.client_id.0),
        client_name,
        peer_addr: peer_addr.clone(),
        transport,
        mode: SessionMode::Unknown,
        connected_at: Utc::now(),
        bytes_up: bytes_up.clone(),
        bytes_down: bytes_down.clone(),
    };
    let session_id = session_keys.session_id;
    ctx.state
        .connections
        .write()
        .await
        .insert(session_id, conn_info);

    let result = handle_session_with_challenge(
        session_keys,
        read,
        write,
        dns_cache,
        forward_config,
        ctx.clone(),
        bytes_up,
        bytes_down,
    )
    .await;

    ctx.state.connections.write().await.remove(&session_id);
    match &result {
        Ok(()) => info!(
            session_id = %session_id,
            client_name = %display_name,
            peer = %peer_addr,
            "Client disconnected"
        ),
        Err(e) => warn!(
            session_id = %session_id,
            client_name = %display_name,
            peer = %peer_addr,
            error = %e,
            "Client disconnected with error"
        ),
    }
    result
}

/// Verify challenge response from the first data frame, then proceed to handle the session.
#[allow(clippy::too_many_arguments)]
async fn handle_session_with_challenge<R, W>(
    session_keys: SessionKeys,
    mut tunnel_read: R,
    tunnel_write: W,
    dns_cache: DnsCache,
    forward_config: PortForwardingConfig,
    ctx: ServerContext,
    bytes_up: Arc<AtomicU64>,
    bytes_down: Arc<AtomicU64>,
) -> Result<()>
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let cipher = create_cipher(session_keys.cipher_suite, &session_keys.session_key);

    // First frame must be ChallengeResponse
    let frame_buf = util::read_framed(&mut tunnel_read).await?;
    let (plaintext, _nonce) = decrypt_frame(cipher.as_ref(), &frame_buf)?;
    let frame = decode_data_frame(&plaintext)?;

    match frame.command {
        Command::ChallengeResponse { hash } => {
            // Verify challenge response
            if let Some(ref challenge) = session_keys.challenge {
                let expected: [u8; 32] = blake3::hash(challenge).into();
                if !util::ct_eq(&hash, &expected) {
                    return Err(anyhow::anyhow!("Invalid challenge response"));
                }
            } else {
                return Err(anyhow::anyhow!("No challenge to verify"));
            }
        }
        _ => {
            return Err(anyhow::anyhow!(
                "Expected ChallengeResponse as first frame, got cmd={}",
                frame.command.cmd_byte()
            ));
        }
    }

    // Now read the actual first command frame (Connect, RegisterForward, etc.)
    handle_session(
        session_keys,
        tunnel_read,
        tunnel_write,
        dns_cache,
        forward_config,
        ctx,
        bytes_up,
        bytes_down,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn handle_session<R, W>(
    session_keys: SessionKeys,
    mut tunnel_read: R,
    tunnel_write: W,
    dns_cache: DnsCache,
    forward_config: PortForwardingConfig,
    ctx: ServerContext,
    bytes_up: Arc<AtomicU64>,
    bytes_down: Arc<AtomicU64>,
) -> Result<()>
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let state = &ctx.state;
    let cipher = create_cipher(session_keys.cipher_suite, &session_keys.session_key);

    // Read first encrypted frame to determine session mode
    let frame_buf = util::read_framed(&mut tunnel_read).await?;

    let (plaintext, _nonce) = decrypt_frame(cipher.as_ref(), &frame_buf)?;
    let frame = decode_data_frame(&plaintext)?;

    match frame.command {
        Command::Connect(ref dest) => {
            // Check routing rules
            if !check_routing_rules(state, dest).await {
                warn!(dest = %dest, "Connection blocked by routing rule");
                return Err(anyhow::anyhow!("Blocked by routing rule"));
            }

            // Update connection mode
            if let Some(conn) = state
                .connections
                .write()
                .await
                .get_mut(&session_keys.session_id)
            {
                conn.mode = SessionMode::Proxy;
            }

            info!(dest = %dest, "Connecting to destination");
            let outbound = outbound::connect(dest, &dns_cache).await?;

            // Use rate-limited relay if client has bandwidth/quota config
            let client_id_str = session_keys.client_id.0.to_string();
            relay::relay_encrypted_with_limits(
                tunnel_read,
                tunnel_write,
                outbound,
                cipher,
                session_keys,
                state.metrics.clone(),
                bytes_up,
                bytes_down,
                client_id_str,
                ctx.bandwidth.clone(),
                ctx.quotas.clone(),
            )
            .await
        }
        Command::RegisterForward {
            remote_port,
            ref name,
        } => {
            if let Some(conn) = state
                .connections
                .write()
                .await
                .get_mut(&session_keys.session_id)
            {
                conn.mode = SessionMode::Forward;
            }

            info!(port = remote_port, name = %name, "Client requesting port forwarding mode");
            forward::run_forward_session_with_first_command(
                tunnel_read,
                tunnel_write,
                cipher,
                session_keys,
                forward_config,
                frame,
                state.metrics.clone(),
                bytes_up,
                bytes_down,
            )
            .await
        }
        Command::DnsQuery { query_id, data } => {
            // Handle encrypted DNS query: forward to upstream DNS and return response
            let upstream = {
                let cfg = state.config.read().await;
                cfg.dns_upstream.clone()
            };
            info!(query_id, upstream = %upstream, "Handling DNS tunnel query");
            let dns_response = match tokio::net::UdpSocket::bind("0.0.0.0:0").await {
                Ok(sock) => {
                    let _ = sock.send_to(&data, &upstream).await;
                    let mut buf = vec![0u8; 4096];
                    match tokio::time::timeout(
                        std::time::Duration::from_secs(5),
                        sock.recv_from(&mut buf),
                    )
                    .await
                    {
                        Ok(Ok((n, _))) => buf[..n].to_vec(),
                        _ => Vec::new(),
                    }
                }
                Err(_) => Vec::new(),
            };
            let cipher = create_cipher(session_keys.cipher_suite, &session_keys.session_key);
            let mut session_keys = session_keys;
            let response_frame = DataFrame {
                command: Command::DnsResponse {
                    query_id,
                    data: dns_response,
                },
                flags: 0,
                stream_id: 0,
            };
            let frame_bytes = encode_data_frame(&response_frame);
            let nonce = session_keys.next_server_nonce();
            let encrypted = encrypt_frame(cipher.as_ref(), &nonce, &frame_bytes)?;
            let mut tunnel_write = tunnel_write;
            let len = (encrypted.len() as u16).to_be_bytes();
            tokio::io::AsyncWriteExt::write_all(&mut tunnel_write, &len).await?;
            tokio::io::AsyncWriteExt::write_all(&mut tunnel_write, &encrypted).await?;
            Ok(())
        }
        Command::SpeedTest {
            direction,
            duration_secs,
            ..
        } => {
            info!(direction, duration_secs, "Speed test requested");
            // Speed test: server sends random data for the specified duration
            let cipher = create_cipher(session_keys.cipher_suite, &session_keys.session_key);
            let mut session_keys = session_keys;
            let mut tunnel_write = tunnel_write;
            if direction == 0 {
                // Download test: server sends data to client
                let start = std::time::Instant::now();
                let duration = std::time::Duration::from_secs(duration_secs as u64);
                let chunk = vec![0xABu8; 8192];
                while start.elapsed() < duration {
                    let frame = DataFrame {
                        command: Command::Data(chunk.clone()),
                        flags: 0,
                        stream_id: 0,
                    };
                    let frame_bytes = encode_data_frame(&frame);
                    let nonce = session_keys.next_server_nonce();
                    match encrypt_frame(cipher.as_ref(), &nonce, &frame_bytes) {
                        Ok(encrypted) => {
                            let len = (encrypted.len() as u16).to_be_bytes();
                            if tokio::io::AsyncWriteExt::write_all(&mut tunnel_write, &len)
                                .await
                                .is_err()
                            {
                                break;
                            }
                            if tokio::io::AsyncWriteExt::write_all(&mut tunnel_write, &encrypted)
                                .await
                                .is_err()
                            {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
            }
            // Send close frame to indicate speed test complete
            let close_frame = DataFrame {
                command: Command::Close,
                flags: 0,
                stream_id: 0,
            };
            let frame_bytes = encode_data_frame(&close_frame);
            let nonce = session_keys.next_server_nonce();
            if let Ok(encrypted) = encrypt_frame(cipher.as_ref(), &nonce, &frame_bytes) {
                let len = (encrypted.len() as u16).to_be_bytes();
                let _ = tokio::io::AsyncWriteExt::write_all(&mut tunnel_write, &len).await;
                let _ = tokio::io::AsyncWriteExt::write_all(&mut tunnel_write, &encrypted).await;
            }
            Ok(())
        }
        Command::UdpAssociate { .. } => {
            if let Some(conn) = state
                .connections
                .write()
                .await
                .get_mut(&session_keys.session_id)
            {
                conn.mode = SessionMode::UdpRelay;
            }

            info!("Client requesting UDP relay mode");
            udp_relay::run_udp_relay_session(
                tunnel_read,
                tunnel_write,
                cipher,
                session_keys,
                frame,
                state.metrics.clone(),
                bytes_up,
                bytes_down,
                None, // FEC config: negotiated out-of-band or from server config
            )
            .await
        }
        other => Err(anyhow::anyhow!(
            "Expected Connect, RegisterForward, or UdpAssociate, got cmd={}",
            other.cmd_byte()
        )),
    }
}

/// Check destination against routing rules. Returns true if allowed.
async fn check_routing_rules(state: &ServerState, dest: &ProxyDestination) -> bool {
    let rules = state.routing_rules.read().await;
    if rules.is_empty() {
        return true; // No rules = allow all
    }

    let mut sorted: Vec<_> = rules.iter().filter(|r| r.enabled).collect();
    sorted.sort_by_key(|r| r.priority);

    for rule in sorted {
        let matches = match &rule.condition {
            RuleCondition::All => true,
            RuleCondition::DomainExact(domain) => match &dest.address {
                ProxyAddress::Domain(d) => d.eq_ignore_ascii_case(domain),
                _ => false,
            },
            RuleCondition::DomainMatch(pattern) => match &dest.address {
                ProxyAddress::Domain(d) => domain_glob_match(pattern, d),
                _ => false,
            },
            RuleCondition::IpCidr(cidr) => match &dest.address {
                ProxyAddress::Ipv4(ip) => cidr_match_v4(cidr, *ip),
                ProxyAddress::Ipv6(_) => false, // simplified
                ProxyAddress::Domain(_) => false,
            },
            RuleCondition::PortRange(start, end) => dest.port >= *start && dest.port <= *end,
        };

        if matches {
            return rule.action == RuleAction::Allow;
        }
    }

    true // Default: allow if no rule matched
}

fn domain_glob_match(pattern: &str, domain: &str) -> bool {
    if let Some(suffix) = pattern.strip_prefix("*.") {
        domain.ends_with(suffix) || domain.eq_ignore_ascii_case(suffix)
    } else {
        domain.eq_ignore_ascii_case(pattern)
    }
}

fn cidr_match_v4(cidr: &str, ip: std::net::Ipv4Addr) -> bool {
    let Some((network, mask)) = prisma_core::router::parse_cidr_v4(cidr) else {
        return false;
    };
    (u32::from(ip) & mask) == network
}
