use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use anyhow::Result;
use chrono::Utc;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tracing::{info, warn};
use prisma_core::cache::DnsCache;
use prisma_core::config::server::{PortForwardingConfig, RuleAction, RuleCondition};
use prisma_core::crypto::aead::create_cipher;
use prisma_core::protocol::codec::*;
use prisma_core::protocol::handshake::ServerHandshake;
use prisma_core::protocol::types::*;
use prisma_core::types::{ProxyAddress, ProxyDestination};
use prisma_core::util;

use prisma_core::state::{ConnectionInfo, ServerState, SessionMode, Transport};

use crate::auth::AuthStore;
use crate::forward;
use crate::outbound;
use crate::relay;

/// Handle an incoming TCP connection through the PrismaVeil protocol.
pub async fn handle_tcp_connection(
    mut stream: TcpStream,
    auth: AuthStore,
    dns_cache: DnsCache,
    forward_config: PortForwardingConfig,
    state: ServerState,
    peer_addr: String,
) -> Result<()> {
    let session_keys = {
        let (mut read, mut write) = stream.split();
        match perform_handshake(&mut read, &mut write, &auth).await {
            Ok(keys) => keys,
            Err(e) => {
                state
                    .metrics
                    .handshake_failures
                    .fetch_add(1, Ordering::Relaxed);
                return Err(e);
            }
        }
    };
    info!(session_id = %session_keys.session_id, "Handshake complete (TCP)");

    // Register connection in state
    let bytes_up = Arc::new(AtomicU64::new(0));
    let bytes_down = Arc::new(AtomicU64::new(0));
    let conn_info = ConnectionInfo {
        session_id: session_keys.session_id,
        client_id: Some(session_keys.client_id.0),
        client_name: None,
        peer_addr,
        transport: Transport::Tcp,
        mode: SessionMode::Unknown,
        connected_at: Utc::now(),
        bytes_up: bytes_up.clone(),
        bytes_down: bytes_down.clone(),
    };
    let session_id = session_keys.session_id;
    state
        .connections
        .write()
        .await
        .insert(session_id, conn_info);

    let (read, write) = stream.into_split();
    let result = handle_session(
        session_keys,
        read,
        write,
        dns_cache,
        forward_config,
        state.clone(),
        bytes_up,
        bytes_down,
    )
    .await;

    // Remove connection from state
    state.connections.write().await.remove(&session_id);
    result
}

/// Handle an incoming QUIC bidirectional stream.
pub async fn handle_quic_stream(
    mut send: quinn::SendStream,
    mut recv: quinn::RecvStream,
    auth: AuthStore,
    dns_cache: DnsCache,
    forward_config: PortForwardingConfig,
    state: ServerState,
    peer_addr: String,
) -> Result<()> {
    let session_keys = match perform_handshake(&mut recv, &mut send, &auth).await {
        Ok(keys) => keys,
        Err(e) => {
            state
                .metrics
                .handshake_failures
                .fetch_add(1, Ordering::Relaxed);
            return Err(e);
        }
    };
    info!(session_id = %session_keys.session_id, "Handshake complete (QUIC)");

    let bytes_up = Arc::new(AtomicU64::new(0));
    let bytes_down = Arc::new(AtomicU64::new(0));
    let conn_info = ConnectionInfo {
        session_id: session_keys.session_id,
        client_id: Some(session_keys.client_id.0),
        client_name: None,
        peer_addr,
        transport: Transport::Quic,
        mode: SessionMode::Unknown,
        connected_at: Utc::now(),
        bytes_up: bytes_up.clone(),
        bytes_down: bytes_down.clone(),
    };
    let session_id = session_keys.session_id;
    state
        .connections
        .write()
        .await
        .insert(session_id, conn_info);

    let result = handle_session(
        session_keys,
        recv,
        send,
        dns_cache,
        forward_config,
        state.clone(),
        bytes_up,
        bytes_down,
    )
    .await;

    state.connections.write().await.remove(&session_id);
    result
}

/// Unified handshake over any AsyncRead + AsyncWrite pair.
async fn perform_handshake<R, W>(
    reader: &mut R,
    writer: &mut W,
    auth: &AuthStore,
) -> Result<SessionKeys>
where
    R: AsyncReadExt + Unpin,
    W: AsyncWriteExt + Unpin,
{
    let client_hello_buf = util::read_framed(reader).await?;

    let (server_hello_bytes, server_state) =
        ServerHandshake::process_client_hello(&client_hello_buf)?;

    util::write_framed(writer, &server_hello_bytes).await?;

    let client_auth_buf = util::read_framed(reader).await?;

    let (accept_bytes, session_keys) = server_state.process_client_auth(&client_auth_buf, auth)?;

    util::write_framed(writer, &accept_bytes).await?;

    Ok(session_keys)
}

async fn handle_session<R, W>(
    session_keys: SessionKeys,
    mut tunnel_read: R,
    tunnel_write: W,
    dns_cache: DnsCache,
    forward_config: PortForwardingConfig,
    state: ServerState,
    bytes_up: Arc<AtomicU64>,
    bytes_down: Arc<AtomicU64>,
) -> Result<()>
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let cipher = create_cipher(session_keys.cipher_suite, &session_keys.session_key);

    // Read first encrypted frame to determine session mode
    let frame_buf = util::read_framed(&mut tunnel_read).await?;

    let (plaintext, _nonce) = decrypt_frame(cipher.as_ref(), &frame_buf)?;
    let frame = decode_data_frame(&plaintext)?;

    match frame.command {
        Command::Connect(ref dest) => {
            // Check routing rules
            if !check_routing_rules(&state, dest).await {
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
            relay::relay_encrypted(
                tunnel_read,
                tunnel_write,
                outbound,
                cipher,
                session_keys,
                state.metrics.clone(),
                bytes_up,
                bytes_down,
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
        other => Err(anyhow::anyhow!(
            "Expected Connect or RegisterForward, got cmd={}",
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
            RuleCondition::PortRange(start, end) => {
                dest.port >= *start && dest.port <= *end
            }
        };

        if matches {
            return rule.action == RuleAction::Allow;
        }
    }

    true // Default: allow if no rule matched
}

fn domain_glob_match(pattern: &str, domain: &str) -> bool {
    if let Some(suffix) = pattern.strip_prefix("*.") {
        domain.ends_with(suffix)
            || domain.eq_ignore_ascii_case(suffix)
    } else {
        domain.eq_ignore_ascii_case(pattern)
    }
}

fn cidr_match_v4(cidr: &str, ip: std::net::Ipv4Addr) -> bool {
    let parts: Vec<&str> = cidr.split('/').collect();
    if parts.len() != 2 {
        return false;
    }
    let Ok(network) = parts[0].parse::<std::net::Ipv4Addr>() else {
        return false;
    };
    let Ok(prefix_len) = parts[1].parse::<u32>() else {
        return false;
    };
    if prefix_len > 32 {
        return false;
    }
    let mask = if prefix_len == 0 {
        0u32
    } else {
        !0u32 << (32 - prefix_len)
    };
    let ip_bits = u32::from(ip);
    let net_bits = u32::from(network);
    (ip_bits & mask) == (net_bits & mask)
}
