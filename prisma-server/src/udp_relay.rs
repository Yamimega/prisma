use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

use anyhow::Result;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::UdpSocket;
use tokio::sync::Mutex;
use tracing::{debug, info, trace, warn};

use prisma_core::crypto::aead::AeadCipher;
use prisma_core::fec::{decode_fec_header, encode_fec_header, FecConfig, FecDecoder, FecEncoder};
use prisma_core::protocol::codec::*;
use prisma_core::protocol::types::*;
use prisma_core::types::MAX_FRAME_SIZE;

use prisma_core::state::ServerMetrics;

/// Maps association ID → UDP socket and its abort handle.
type AssocMap = Arc<Mutex<HashMap<u32, Arc<UdpSocket>>>>;

/// Shared state for a UDP relay session.
struct UdpRelayCtx<W> {
    tunnel_write: Arc<Mutex<W>>,
    session_keys: Arc<Mutex<SessionKeys>>,
    cipher: Arc<dyn AeadCipher>,
    next_assoc_id: Arc<AtomicU32>,
    associations: AssocMap,
    metrics: Arc<ServerMetrics>,
    bytes_up: Arc<AtomicU64>,
    bytes_down: Arc<AtomicU64>,
    fec_encoder: Option<Arc<Mutex<FecEncoder>>>,
    fec_decoder: Option<Arc<Mutex<FecDecoder>>>,
    fec_config: Option<FecConfig>,
    /// Sequence counter for tracking data shard indices on the receive side.
    recv_seq: Arc<Mutex<u8>>,
}

impl<W> Clone for UdpRelayCtx<W> {
    fn clone(&self) -> Self {
        Self {
            tunnel_write: self.tunnel_write.clone(),
            session_keys: self.session_keys.clone(),
            cipher: self.cipher.clone(),
            next_assoc_id: self.next_assoc_id.clone(),
            associations: self.associations.clone(),
            metrics: self.metrics.clone(),
            bytes_up: self.bytes_up.clone(),
            bytes_down: self.bytes_down.clone(),
            fec_encoder: self.fec_encoder.clone(),
            fec_decoder: self.fec_decoder.clone(),
            fec_config: self.fec_config.clone(),
            recv_seq: self.recv_seq.clone(),
        }
    }
}

/// Run a UDP relay session. The first frame (UdpAssociate) has already been parsed.
#[allow(clippy::too_many_arguments)]
pub async fn run_udp_relay_session<R, W>(
    tunnel_read: R,
    tunnel_write: W,
    cipher: Box<dyn AeadCipher>,
    session_keys: SessionKeys,
    first_frame: DataFrame,
    metrics: Arc<ServerMetrics>,
    bytes_up: Arc<AtomicU64>,
    bytes_down: Arc<AtomicU64>,
    fec_config: Option<FecConfig>,
) -> Result<()>
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let fec_encoder = fec_config.as_ref().map(|cfg| {
        Arc::new(Mutex::new(FecEncoder::new(
            cfg.data_shards,
            cfg.parity_shards,
        )))
    });
    let fec_decoder = fec_config.as_ref().map(|cfg| {
        Arc::new(Mutex::new(FecDecoder::new(
            cfg.data_shards,
            cfg.parity_shards,
        )))
    });

    let ctx = UdpRelayCtx {
        cipher: Arc::from(cipher),
        tunnel_write: Arc::new(Mutex::new(tunnel_write)),
        session_keys: Arc::new(Mutex::new(session_keys)),
        next_assoc_id: Arc::new(AtomicU32::new(1)),
        associations: Arc::new(Mutex::new(HashMap::new())),
        metrics,
        bytes_up,
        bytes_down,
        fec_encoder,
        fec_decoder,
        fec_config,
        recv_seq: Arc::new(Mutex::new(0)),
    };

    // Process the first UdpAssociate frame
    dispatch_frame(first_frame, &ctx).await?;

    // Continue reading frames
    read_loop(tunnel_read, &ctx).await
}

/// Read encrypted frames from the tunnel and dispatch them.
async fn read_loop<R, W>(mut tunnel_read: R, ctx: &UdpRelayCtx<W>) -> Result<()>
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let mut frame_buf = Vec::with_capacity(MAX_FRAME_SIZE);
    loop {
        let mut len_buf = [0u8; 2];
        if tunnel_read.read_exact(&mut len_buf).await.is_err() {
            break;
        }
        let frame_len = u16::from_be_bytes(len_buf) as usize;
        if frame_len > MAX_FRAME_SIZE {
            warn!(size = frame_len, "UDP relay: frame too large");
            break;
        }
        frame_buf.resize(frame_len, 0);
        if tunnel_read
            .read_exact(&mut frame_buf[..frame_len])
            .await
            .is_err()
        {
            break;
        }

        let frame_bytes = frame_len as u64 + 2;
        ctx.bytes_up.fetch_add(frame_bytes, Ordering::Relaxed);
        ctx.metrics
            .total_bytes_up
            .fetch_add(frame_bytes, Ordering::Relaxed);

        let (plaintext, _nonce) = match decrypt_frame(ctx.cipher.as_ref(), &frame_buf[..frame_len])
        {
            Ok(r) => r,
            Err(e) => {
                warn!("UDP relay decrypt error: {}", e);
                break;
            }
        };

        let frame = match decode_data_frame(&plaintext) {
            Ok(f) => f,
            Err(e) => {
                warn!("UDP relay frame decode error: {}", e);
                break;
            }
        };

        dispatch_frame(frame, ctx).await?;
    }

    debug!("UDP relay session ended");
    Ok(())
}

/// Dispatch a single decoded frame.
async fn dispatch_frame<W: AsyncWrite + Unpin + Send + 'static>(
    frame: DataFrame,
    ctx: &UdpRelayCtx<W>,
) -> Result<()> {
    match frame.command {
        Command::UdpAssociate { .. } => {
            let assoc_id = ctx.next_assoc_id.fetch_add(1, Ordering::Relaxed);

            // Bind a UDP socket on an ephemeral port
            let socket = UdpSocket::bind("0.0.0.0:0").await?;
            let local_addr = socket.local_addr()?;
            info!(assoc_id, addr = %local_addr, "UDP association created");

            let socket = Arc::new(socket);
            ctx.associations
                .lock()
                .await
                .insert(assoc_id, socket.clone());

            // Send ForwardReady to acknowledge the association
            let ready_port = local_addr.port();
            send_frame(
                ctx,
                Command::ForwardReady {
                    remote_port: ready_port,
                    success: true,
                },
                assoc_id,
            )
            .await?;

            // Spawn a task to read incoming UDP datagrams and relay them back
            let ctx = ctx.clone();
            tokio::spawn(async move {
                if let Err(e) = udp_recv_loop(socket, assoc_id, &ctx).await {
                    warn!(assoc_id, error = %e, "UDP recv loop error");
                }
                ctx.associations.lock().await.remove(&assoc_id);
                debug!(assoc_id, "UDP association removed");
            });
        }
        Command::UdpData {
            assoc_id,
            addr_type,
            dest_addr,
            dest_port,
            payload,
            ..
        } => {
            if frame.flags & FLAG_FEC != 0 {
                // FEC parity shard from client: feed to decoder
                if let Some(ref decoder) = ctx.fec_decoder {
                    if payload.len() < 4 {
                        warn!("FEC shard too short");
                        return Ok(());
                    }
                    let header: [u8; 4] = payload[..4].try_into().unwrap();
                    let (group_id, shard_index, _total) = decode_fec_header(&header);
                    let shard_data = &payload[4..];

                    let mut dec = decoder.lock().await;
                    if let Some(recovered_shards) = dec.add_shard(group_id, shard_index, shard_data)
                    {
                        trace!(
                            group_id,
                            count = recovered_shards.len(),
                            "FEC recovered shards on server"
                        );
                        // Recovered shards are raw payloads; best-effort recovery.
                        // In a full implementation we'd forward recovered data to the target.
                    }
                }
            } else {
                // Regular data shard: feed to FEC decoder for tracking, then forward
                if let Some(ref decoder) = ctx.fec_decoder {
                    if let Some(ref cfg) = ctx.fec_config {
                        let mut seq = ctx.recv_seq.lock().await;
                        let shard_index = *seq;
                        let group_id = (shard_index as u16) / (cfg.data_shards as u16);
                        let index_in_group = shard_index % (cfg.data_shards as u8);

                        let mut dec = decoder.lock().await;
                        let _ = dec.add_shard(group_id, index_in_group, &payload);
                        *seq = seq.wrapping_add(1);
                    }
                }

                let socket = ctx.associations.lock().await.get(&assoc_id).cloned();
                if let Some(socket) = socket {
                    // Resolve destination address
                    let dest = resolve_udp_dest(addr_type, &dest_addr, dest_port);
                    match dest {
                        Ok(addr) => {
                            if let Err(e) = socket.send_to(&payload, addr).await {
                                warn!(assoc_id, dest = %addr, error = %e, "UDP send failed");
                            }
                        }
                        Err(e) => {
                            warn!(assoc_id, error = %e, "Failed to resolve UDP destination");
                        }
                    }
                } else {
                    warn!(assoc_id, "UDP data for unknown association");
                }
            }
        }
        Command::Close => {
            // Close all associations
            ctx.associations.lock().await.clear();
        }
        Command::Ping(seq) => {
            send_frame(ctx, Command::Pong(seq), frame.stream_id).await?;
        }
        _ => {}
    }
    Ok(())
}

/// Receive UDP datagrams from the bound socket and relay them back through the tunnel.
async fn udp_recv_loop<W: AsyncWrite + Unpin + Send + 'static>(
    socket: Arc<UdpSocket>,
    assoc_id: u32,
    ctx: &UdpRelayCtx<W>,
) -> Result<()> {
    let mut buf = vec![0u8; 65535];
    loop {
        let (n, peer) = match socket.recv_from(&mut buf).await {
            Ok(r) => r,
            Err(e) => {
                warn!(assoc_id, error = %e, "UDP recv error");
                break;
            }
        };

        // Build address info for the response
        let (addr_type, dest_addr) = match peer {
            SocketAddr::V4(v4) => (0x01, v4.ip().octets().to_vec()),
            SocketAddr::V6(v6) => (0x04, v6.ip().octets().to_vec()),
        };

        let payload = buf[..n].to_vec();

        let cmd = Command::UdpData {
            assoc_id,
            frag: 0,
            addr_type,
            dest_addr: dest_addr.clone(),
            dest_port: peer.port(),
            payload: payload.clone(),
        };

        if let Err(e) = send_frame(ctx, cmd, 0).await {
            warn!(assoc_id, error = %e, "Failed to send UDP data frame");
            break;
        }

        // FEC: feed the payload to the encoder and send parity shards when group completes
        if let Some(ref encoder) = ctx.fec_encoder {
            let mut enc = encoder.lock().await;
            if let Some(group) = enc.add_shard(&payload) {
                let total = group.data_shards + group.parity_shards;
                // Send only parity shards (data shards already sent as regular frames)
                for i in
                    group.data_shards as usize..(group.data_shards + group.parity_shards) as usize
                {
                    let header = encode_fec_header(group.group_id, i as u8, total);
                    let mut fec_payload = Vec::with_capacity(4 + group.shards[i].len());
                    fec_payload.extend_from_slice(&header);
                    fec_payload.extend_from_slice(&group.shards[i]);

                    let parity_cmd = Command::UdpData {
                        assoc_id,
                        frag: 0,
                        addr_type,
                        dest_addr: vec![],
                        dest_port: 0,
                        payload: fec_payload,
                    };

                    if let Err(e) = send_frame_with_flags(ctx, parity_cmd, 0, FLAG_FEC).await {
                        warn!(
                            assoc_id,
                            error = %e,
                            "Failed to send FEC parity frame"
                        );
                        break;
                    }
                }
            }
        }
    }
    Ok(())
}

/// Resolve a UDP destination from address type and raw bytes.
fn resolve_udp_dest(addr_type: u8, addr: &[u8], port: u16) -> Result<SocketAddr> {
    match addr_type {
        0x01 => {
            // IPv4
            if addr.len() != 4 {
                return Err(anyhow::anyhow!("Invalid IPv4 address length"));
            }
            let ip = std::net::Ipv4Addr::new(addr[0], addr[1], addr[2], addr[3]);
            Ok(SocketAddr::new(ip.into(), port))
        }
        0x03 => {
            // Domain — resolve synchronously via blocking task
            let domain = String::from_utf8(addr.to_vec())
                .map_err(|_| anyhow::anyhow!("Invalid domain encoding"))?;
            let addr_str = format!("{}:{}", domain, port);
            // Use std::net for sync DNS resolution (in async context, acceptable for UDP)
            use std::net::ToSocketAddrs;
            let resolved = addr_str
                .to_socket_addrs()
                .map_err(|e| anyhow::anyhow!("DNS resolution failed for {}: {}", domain, e))?
                .next()
                .ok_or_else(|| anyhow::anyhow!("No addresses resolved for {}", domain))?;
            Ok(resolved)
        }
        0x04 => {
            // IPv6
            if addr.len() != 16 {
                return Err(anyhow::anyhow!("Invalid IPv6 address length"));
            }
            let mut octets = [0u8; 16];
            octets.copy_from_slice(addr);
            let ip = std::net::Ipv6Addr::from(octets);
            Ok(SocketAddr::new(ip.into(), port))
        }
        _ => Err(anyhow::anyhow!("Unsupported address type: {}", addr_type)),
    }
}

/// Encrypt and send a single frame through the tunnel.
async fn send_frame<W: AsyncWrite + Unpin>(
    ctx: &UdpRelayCtx<W>,
    command: Command,
    stream_id: u32,
) -> Result<()> {
    send_frame_with_flags(ctx, command, stream_id, 0).await
}

/// Encrypt and send a single frame through the tunnel with explicit flags.
async fn send_frame_with_flags<W: AsyncWrite + Unpin>(
    ctx: &UdpRelayCtx<W>,
    command: Command,
    stream_id: u32,
    flags: u16,
) -> Result<()> {
    let frame = DataFrame {
        command,
        flags,
        stream_id,
    };
    let frame_bytes = encode_data_frame(&frame);
    let nonce = ctx.session_keys.lock().await.next_server_nonce();
    let encrypted = encrypt_frame(ctx.cipher.as_ref(), &nonce, &frame_bytes)?;

    let enc_len = encrypted.len() as u64 + 2;
    ctx.bytes_down.fetch_add(enc_len, Ordering::Relaxed);
    ctx.metrics
        .total_bytes_down
        .fetch_add(enc_len, Ordering::Relaxed);

    let len = (encrypted.len() as u16).to_be_bytes();
    let mut tw = ctx.tunnel_write.lock().await;
    tw.write_all(&len).await?;
    tw.write_all(&encrypted).await?;
    Ok(())
}
