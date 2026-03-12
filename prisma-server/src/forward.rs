use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

use anyhow::Result;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Mutex};
use tracing::{debug, info, warn};

use prisma_core::config::server::PortForwardingConfig;
use prisma_core::crypto::aead::AeadCipher;
use prisma_core::protocol::codec::*;
use prisma_core::protocol::types::*;
use prisma_core::types::MAX_FRAME_SIZE;

use prisma_core::state::ServerMetrics;

type StreamMap = Arc<Mutex<HashMap<u32, mpsc::Sender<Vec<u8>>>>>;

/// Shared state for a multiplexed forward session.
struct ForwardCtx<W> {
    tunnel_write: Arc<Mutex<W>>,
    session_keys: Arc<Mutex<SessionKeys>>,
    cipher: Arc<dyn AeadCipher>,
    next_stream_id: Arc<AtomicU32>,
    streams: StreamMap,
    metrics: Arc<ServerMetrics>,
    bytes_up: Arc<AtomicU64>,
    bytes_down: Arc<AtomicU64>,
}

impl<W> Clone for ForwardCtx<W> {
    fn clone(&self) -> Self {
        Self {
            tunnel_write: self.tunnel_write.clone(),
            session_keys: self.session_keys.clone(),
            cipher: self.cipher.clone(),
            next_stream_id: self.next_stream_id.clone(),
            streams: self.streams.clone(),
            metrics: self.metrics.clone(),
            bytes_up: self.bytes_up.clone(),
            bytes_down: self.bytes_down.clone(),
        }
    }
}

/// Entry point when the first command has already been parsed by the handler.
pub async fn run_forward_session_with_first_command<R, W>(
    tunnel_read: R,
    tunnel_write: W,
    cipher: Box<dyn AeadCipher>,
    session_keys: SessionKeys,
    forward_config: PortForwardingConfig,
    first_frame: DataFrame,
    metrics: Arc<ServerMetrics>,
    bytes_up: Arc<AtomicU64>,
    bytes_down: Arc<AtomicU64>,
) -> Result<()>
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let ctx = ForwardCtx {
        cipher: Arc::from(cipher),
        tunnel_write: Arc::new(Mutex::new(tunnel_write)),
        session_keys: Arc::new(Mutex::new(session_keys)),
        next_stream_id: Arc::new(AtomicU32::new(1)),
        streams: Arc::new(Mutex::new(HashMap::new())),
        metrics,
        bytes_up,
        bytes_down,
    };

    // Process the first frame
    dispatch_frame(first_frame, &forward_config, &ctx).await?;

    // Continue reading remaining frames
    read_loop(tunnel_read, &forward_config, &ctx).await
}

/// Manages multiplexed port forwarding over an encrypted tunnel.
async fn read_loop<R, W>(
    mut tunnel_read: R,
    forward_config: &PortForwardingConfig,
    ctx: &ForwardCtx<W>,
) -> Result<()>
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let mut frame_buf = Vec::with_capacity(MAX_FRAME_SIZE);
    loop {
        // Read length-prefixed encrypted frame
        let mut len_buf = [0u8; 2];
        if tunnel_read.read_exact(&mut len_buf).await.is_err() {
            break;
        }
        let frame_len = u16::from_be_bytes(len_buf) as usize;
        if frame_len > MAX_FRAME_SIZE {
            warn!(size = frame_len, "Frame too large from client");
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

        // Count upstream bytes
        let frame_bytes = frame_len as u64 + 2;
        ctx.bytes_up.fetch_add(frame_bytes, Ordering::Relaxed);
        ctx.metrics.total_bytes_up.fetch_add(frame_bytes, Ordering::Relaxed);

        let (plaintext, _nonce) = match decrypt_frame(ctx.cipher.as_ref(), &frame_buf[..frame_len])
        {
            Ok(r) => r,
            Err(e) => {
                warn!("Decrypt error: {}", e);
                break;
            }
        };

        let frame = match decode_data_frame(&plaintext) {
            Ok(f) => f,
            Err(e) => {
                warn!("Frame decode error: {}", e);
                break;
            }
        };

        dispatch_frame(frame, forward_config, ctx).await?;
    }

    debug!("Forward session ended");
    Ok(())
}

async fn dispatch_frame<W: AsyncWrite + Unpin + Send + 'static>(
    frame: DataFrame,
    forward_config: &PortForwardingConfig,
    ctx: &ForwardCtx<W>,
) -> Result<()> {
    match frame.command {
        Command::RegisterForward { remote_port, name } => {
            if forward_config.is_port_allowed(remote_port) {
                info!(port = remote_port, name = %name, "Registering port forward");

                send_frame(ctx, Command::ForwardReady { remote_port, success: true }, 0).await?;

                let ctx = ctx.clone();
                tokio::spawn(async move {
                    if let Err(e) = run_forward_listener(remote_port, &ctx).await {
                        warn!(port = remote_port, error = %e, "Forward listener error");
                    }
                });
            } else {
                warn!(port = remote_port, name = %name, "Port forward denied");
                send_frame(ctx, Command::ForwardReady { remote_port, success: false }, 0).await?;
            }
        }
        Command::Data(data) => {
            // Clone the sender outside the lock, then send without holding it
            let tx = ctx.streams.lock().await.get(&frame.stream_id).cloned();
            if let Some(tx) = tx {
                let _ = tx.send(data).await;
            }
        }
        Command::Close => {
            ctx.streams.lock().await.remove(&frame.stream_id);
        }
        Command::Ping(seq) => {
            send_frame(ctx, Command::Pong(seq), frame.stream_id).await?;
        }
        _ => {}
    }
    Ok(())
}

/// Listen on a forwarded port and relay incoming connections through the tunnel.
async fn run_forward_listener<W: AsyncWrite + Unpin + Send + 'static>(
    port: u16,
    ctx: &ForwardCtx<W>,
) -> Result<()> {
    let addr = format!("0.0.0.0:{}", port);
    let listener = TcpListener::bind(&addr).await?;
    info!(addr = %addr, "Forward listener started");

    loop {
        let (inbound, peer) = listener.accept().await?;
        let stream_id = ctx.next_stream_id.fetch_add(1, Ordering::Relaxed);
        debug!(stream_id, peer = %peer, port, "New forwarded connection");

        // Create a channel for data from the tunnel dispatcher to this connection
        let (tx, rx) = mpsc::channel::<Vec<u8>>(64);
        ctx.streams.lock().await.insert(stream_id, tx);

        // Notify the client about this new connection
        if let Err(e) = send_frame(ctx, Command::ForwardConnect { remote_port: port }, stream_id)
            .await
        {
            warn!(stream_id, "Failed to send ForwardConnect: {}", e);
            ctx.streams.lock().await.remove(&stream_id);
            continue;
        }

        // Spawn bidirectional relay for this connection
        let ctx = ctx.clone();
        tokio::spawn(async move {
            relay_forwarded(inbound, rx, &ctx, stream_id).await;
            ctx.streams.lock().await.remove(&stream_id);
            debug!(stream_id, "Forwarded connection ended");
        });
    }
}

/// Relay data between an inbound TCP connection and the encrypted tunnel, using stream_id.
async fn relay_forwarded<W: AsyncWrite + Unpin + Send + 'static>(
    inbound: TcpStream,
    mut from_tunnel: mpsc::Receiver<Vec<u8>>,
    ctx: &ForwardCtx<W>,
    stream_id: u32,
) {
    let (mut tcp_read, mut tcp_write) = inbound.into_split();

    // inbound TCP → tunnel
    let ctx2 = ctx.clone();
    let tcp_to_tunnel = tokio::spawn(async move {
        let mut buf = vec![0u8; 8192];
        loop {
            match tcp_read.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    if send_frame(&ctx2, Command::Data(buf[..n].to_vec()), stream_id)
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        // Send close for this stream
        let _ = send_frame(&ctx2, Command::Close, stream_id).await;
    });

    // tunnel → inbound TCP
    let tunnel_to_tcp = tokio::spawn(async move {
        while let Some(data) = from_tunnel.recv().await {
            if tcp_write.write_all(&data).await.is_err() {
                break;
            }
        }
    });

    tokio::select! {
        _ = tcp_to_tunnel => {},
        _ = tunnel_to_tcp => {},
    }
}

/// Helper: encrypt and send a single frame through the tunnel.
async fn send_frame<W: AsyncWrite + Unpin>(
    ctx: &ForwardCtx<W>,
    command: Command,
    stream_id: u32,
) -> Result<()> {
    let frame = DataFrame {
        command,
        flags: 0,
        stream_id,
    };
    let frame_bytes = encode_data_frame(&frame);
    let nonce = ctx.session_keys.lock().await.next_server_nonce();
    let encrypted = encrypt_frame(ctx.cipher.as_ref(), &nonce, &frame_bytes)?;

    // Count downstream bytes
    let enc_len = encrypted.len() as u64 + 2;
    ctx.bytes_down.fetch_add(enc_len, Ordering::Relaxed);
    ctx.metrics.total_bytes_down.fetch_add(enc_len, Ordering::Relaxed);

    let len = (encrypted.len() as u16).to_be_bytes();
    let mut tw = ctx.tunnel_write.lock().await;
    tw.write_all(&len).await?;
    tw.write_all(&encrypted).await?;
    Ok(())
}
