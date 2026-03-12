use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use tokio::io::{AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, Mutex};
use tracing::{debug, info, warn};

use prisma_core::config::client::PortForwardConfig;
use prisma_core::crypto::aead::{create_cipher, AeadCipher};
use prisma_core::protocol::codec::*;
use prisma_core::protocol::handshake::ClientHandshake;
use prisma_core::protocol::types::*;
use prisma_core::types::MAX_FRAME_SIZE;
use prisma_core::util;

use crate::proxy::ProxyContext;

/// Run the port forwarding client: establish a persistent control tunnel
/// and register all configured port forwards.
pub async fn run_port_forwards(ctx: ProxyContext, forwards: Vec<PortForwardConfig>) -> Result<()> {
    info!(count = forwards.len(), "Starting port forwarding");

    // Establish tunnel
    let mut stream = ctx.connect().await?;

    // Perform handshake
    let handshake = ClientHandshake::new(ctx.client_id, ctx.auth_secret, ctx.cipher_suite);
    let (client_state, hello_bytes) = handshake.start();
    util::write_framed(&mut stream, &hello_bytes).await?;

    let server_hello_buf = util::read_framed(&mut stream).await?;
    let (client_auth_bytes, accept_state) = client_state.process_server_hello(&server_hello_buf)?;
    util::write_framed(&mut stream, &client_auth_bytes).await?;

    let accept_buf = util::read_framed(&mut stream).await?;
    let session_keys = accept_state.process_server_accept(&accept_buf)?;
    info!(session_id = %session_keys.session_id, "Forward tunnel established");

    let cipher: Arc<dyn AeadCipher> = Arc::from(create_cipher(session_keys.cipher_suite, &session_keys.session_key));
    let (mut tunnel_read, tunnel_write) = tokio::io::split(stream);
    let tunnel_write = Arc::new(Mutex::new(tunnel_write));
    let session_keys = Arc::new(Mutex::new(session_keys));

    // Build a map: remote_port → local_addr for routing ForwardConnect
    let port_map: Arc<HashMap<u16, String>> = Arc::new(
        forwards
            .iter()
            .map(|f| (f.remote_port, f.local_addr.clone()))
            .collect(),
    );

    // Register each port forward
    for fwd in &forwards {
        info!(
            name = %fwd.name,
            local = %fwd.local_addr,
            remote_port = fwd.remote_port,
            "Registering port forward"
        );
        send_frame(
            &tunnel_write,
            &session_keys,
            &cipher,
            Command::RegisterForward {
                remote_port: fwd.remote_port,
                name: fwd.name.clone(),
            },
            0,
        )
        .await?;
    }

    // Map of stream_id → sender for data going to local TCP
    let streams: Arc<Mutex<HashMap<u32, mpsc::Sender<Vec<u8>>>>> =
        Arc::new(Mutex::new(HashMap::new()));

    // Read frames from the tunnel and dispatch
    let mut frame_buf = Vec::with_capacity(MAX_FRAME_SIZE);
    loop {
        let mut len_buf = [0u8; 2];
        if tunnel_read.read_exact(&mut len_buf).await.is_err() {
            break;
        }
        let frame_len = u16::from_be_bytes(len_buf) as usize;
        if frame_len > MAX_FRAME_SIZE {
            warn!(size = frame_len, "Frame too large from server");
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

        let (plaintext, _) = match decrypt_frame(cipher.as_ref(), &frame_buf[..frame_len]) {
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

        match frame.command {
            Command::ForwardReady {
                remote_port,
                success,
            } => {
                if success {
                    info!(port = remote_port, "Port forward registered successfully");
                } else {
                    warn!(port = remote_port, "Port forward registration denied by server");
                }
            }
            Command::ForwardConnect { remote_port } => {
                let stream_id = frame.stream_id;
                if let Some(local_addr) = port_map.get(&remote_port) {
                    debug!(stream_id, remote_port, local = %local_addr, "New forwarded connection");

                    let (tx, rx) = mpsc::channel::<Vec<u8>>(64);
                    streams.lock().await.insert(stream_id, tx);

                    let local_addr = local_addr.clone();
                    let tw = tunnel_write.clone();
                    let sk = session_keys.clone();
                    let c = cipher.clone();
                    let st = streams.clone();
                    tokio::spawn(async move {
                        match TcpStream::connect(&local_addr).await {
                            Ok(local_stream) => {
                                relay_local(local_stream, rx, tw, sk, c, stream_id).await;
                            }
                            Err(e) => {
                                warn!(stream_id, local = %local_addr, error = %e, "Failed to connect to local service");
                                let _ = send_frame(&tw, &sk, &c, Command::Close, stream_id).await;
                            }
                        }
                        st.lock().await.remove(&stream_id);
                    });
                } else {
                    warn!(
                        stream_id,
                        remote_port, "ForwardConnect for unknown port"
                    );
                    let _ = send_frame(
                        &tunnel_write,
                        &session_keys,
                        &cipher,
                        Command::Close,
                        stream_id,
                    )
                    .await;
                }
            }
            Command::Data(data) => {
                // Clone the sender outside the lock, then send without holding it
                let tx = streams.lock().await.get(&frame.stream_id).cloned();
                if let Some(tx) = tx {
                    let _ = tx.send(data).await;
                }
            }
            Command::Close => {
                streams.lock().await.remove(&frame.stream_id);
            }
            _ => {}
        }
    }

    debug!("Forward tunnel ended");
    Ok(())
}

/// Relay between a local TCP connection and the tunnel, using stream_id.
async fn relay_local<W: AsyncWrite + Unpin + Send + 'static>(
    local: TcpStream,
    mut from_tunnel: mpsc::Receiver<Vec<u8>>,
    tunnel_write: Arc<Mutex<W>>,
    session_keys: Arc<Mutex<SessionKeys>>,
    cipher: Arc<dyn AeadCipher>,
    stream_id: u32,
) {
    let (mut tcp_read, mut tcp_write) = local.into_split();

    let tw = tunnel_write.clone();
    let sk = session_keys.clone();
    let c = cipher.clone();
    let local_to_tunnel = tokio::spawn(async move {
        let mut buf = vec![0u8; 8192];
        loop {
            match tcp_read.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    if send_frame(&tw, &sk, &c, Command::Data(buf[..n].to_vec()), stream_id)
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        let _ = send_frame(&tw, &sk, &c, Command::Close, stream_id).await;
    });

    let tunnel_to_local = tokio::spawn(async move {
        while let Some(data) = from_tunnel.recv().await {
            if tcp_write.write_all(&data).await.is_err() {
                break;
            }
        }
    });

    tokio::select! {
        _ = local_to_tunnel => {},
        _ = tunnel_to_local => {},
    }
}

/// Helper: encrypt and send a single frame through the tunnel.
async fn send_frame<W: AsyncWrite + Unpin>(
    tunnel_write: &Arc<Mutex<W>>,
    session_keys: &Arc<Mutex<SessionKeys>>,
    cipher: &Arc<dyn AeadCipher>,
    command: Command,
    stream_id: u32,
) -> Result<()> {
    let frame = DataFrame {
        command,
        flags: 0,
        stream_id,
    };
    let frame_bytes = encode_data_frame(&frame);
    let nonce = session_keys.lock().await.next_client_nonce();
    let encrypted = encrypt_frame(cipher.as_ref(), &nonce, &frame_bytes)?;
    let len = (encrypted.len() as u16).to_be_bytes();
    let mut tw = tunnel_write.lock().await;
    tw.write_all(&len).await?;
    tw.write_all(&encrypted).await?;
    Ok(())
}
