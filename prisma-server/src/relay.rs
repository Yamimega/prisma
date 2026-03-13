use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use anyhow::Result;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tracing::{debug, warn};

use prisma_core::crypto::aead::AeadCipher;
use prisma_core::protocol::anti_replay::AntiReplayWindow;
use prisma_core::protocol::codec::*;
use prisma_core::protocol::types::*;
use prisma_core::types::{MAX_FRAME_SIZE, PROTOCOL_VERSION_V2};

use prisma_core::state::ServerMetrics;

use crate::bandwidth::limiter::BandwidthLimiterStore;
use crate::bandwidth::quota::QuotaStore;

/// Bidirectional encrypted relay between tunnel and destination.
#[allow(clippy::too_many_arguments)]
pub async fn relay_encrypted<R, W>(
    mut tunnel_read: R,
    tunnel_write: W,
    outbound: TcpStream,
    cipher: Box<dyn AeadCipher>,
    session_keys: SessionKeys,
    metrics: Arc<ServerMetrics>,
    bytes_up: Arc<AtomicU64>,
    bytes_down: Arc<AtomicU64>,
) -> Result<()>
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let (mut out_read, mut out_write) = outbound.into_split();
    let cipher: Arc<dyn AeadCipher> = Arc::from(cipher);
    let tunnel_write = Arc::new(Mutex::new(tunnel_write));
    let session_keys = Arc::new(Mutex::new(session_keys));

    let cipher_t2d = cipher.clone();
    let tunnel_write_ping = tunnel_write.clone();
    let session_keys_ping = session_keys.clone();
    let metrics_t2d = metrics.clone();
    let bytes_up_t2d = bytes_up.clone();

    // tunnel → destination: decrypt frames with anti-replay, send raw data
    let tunnel_to_dest = tokio::spawn(async move {
        let mut anti_replay = AntiReplayWindow::new();
        let mut frame_buf = Vec::with_capacity(MAX_FRAME_SIZE);

        loop {
            let mut len_buf = [0u8; 2];
            if tunnel_read.read_exact(&mut len_buf).await.is_err() {
                break;
            }
            let frame_len = u16::from_be_bytes(len_buf) as usize;
            if frame_len > MAX_FRAME_SIZE {
                warn!(size = frame_len, max = MAX_FRAME_SIZE, "Frame too large");
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

            // Count upstream bytes (encrypted frame size)
            let frame_bytes = frame_len as u64 + 2;
            bytes_up_t2d.fetch_add(frame_bytes, Ordering::Relaxed);
            metrics_t2d
                .total_bytes_up
                .fetch_add(frame_bytes, Ordering::Relaxed);

            match decrypt_frame(cipher_t2d.as_ref(), &frame_buf[..frame_len]) {
                Ok((plaintext, nonce)) => {
                    // Extract counter from nonce for anti-replay check
                    let counter = nonce_to_counter(&nonce);
                    if let Err(e) = anti_replay.check_and_update(counter) {
                        warn!("Anti-replay check failed: {}", e);
                        break;
                    }

                    match decode_data_frame(&plaintext) {
                        Ok(frame) => match frame.command {
                            Command::Data(data) => {
                                if out_write.write_all(&data).await.is_err() {
                                    break;
                                }
                            }
                            Command::Close => break,
                            Command::Ping(seq) => {
                                let pong = DataFrame {
                                    command: Command::Pong(seq),
                                    flags: 0,
                                    stream_id: frame.stream_id,
                                };
                                let pong_bytes = encode_data_frame(&pong);
                                let nonce = session_keys_ping.lock().await.next_server_nonce();
                                if let Ok(encrypted) =
                                    encrypt_frame(cipher_t2d.as_ref(), &nonce, &pong_bytes)
                                {
                                    let mut tw = tunnel_write_ping.lock().await;
                                    let len = (encrypted.len() as u16).to_be_bytes();
                                    let _ = tw.write_all(&len).await;
                                    let _ = tw.write_all(&encrypted).await;
                                }
                            }
                            _ => {}
                        },
                        Err(e) => {
                            warn!("Frame decode error: {}", e);
                            break;
                        }
                    }
                }
                Err(e) => {
                    warn!("Frame decrypt error: {}", e);
                    break;
                }
            }
        }
    });

    let cipher_d2t = cipher.clone();
    let session_keys_d2t = session_keys.clone();

    // Capture padding config for dest→tunnel direction
    let session_keys_padding = session_keys.clone();

    // destination → tunnel: read raw data, encrypt into frames
    let dest_to_tunnel = tokio::spawn(async move {
        let sk = session_keys_padding.lock().await;
        let use_padding = sk.protocol_version >= PROTOCOL_VERSION_V2;
        let padding_range = sk.padding_range;
        drop(sk);

        let mut buf = vec![0u8; 8192];
        loop {
            match out_read.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    let flags = if use_padding { FLAG_PADDED } else { 0 };
                    let frame = DataFrame {
                        command: Command::Data(buf[..n].to_vec()),
                        flags,
                        stream_id: 0,
                    };
                    let frame_bytes = if use_padding {
                        encode_data_frame_padded(&frame, &padding_range)
                    } else {
                        encode_data_frame(&frame)
                    };
                    let nonce = session_keys_d2t.lock().await.next_server_nonce();
                    match encrypt_frame(cipher_d2t.as_ref(), &nonce, &frame_bytes) {
                        Ok(encrypted) => {
                            let enc_len = encrypted.len() as u64 + 2;
                            bytes_down.fetch_add(enc_len, Ordering::Relaxed);
                            metrics
                                .total_bytes_down
                                .fetch_add(enc_len, Ordering::Relaxed);

                            let mut tw = tunnel_write.lock().await;
                            let len = (encrypted.len() as u16).to_be_bytes();
                            if tw.write_all(&len).await.is_err() {
                                break;
                            }
                            if tw.write_all(&encrypted).await.is_err() {
                                break;
                            }
                        }
                        Err(e) => {
                            warn!("Frame encrypt error: {}", e);
                            break;
                        }
                    }
                }
                Err(_) => break,
            }
        }
    });

    tokio::select! {
        _ = tunnel_to_dest => {},
        _ = dest_to_tunnel => {},
    }

    debug!("Relay session ended");
    Ok(())
}

/// Bidirectional encrypted relay with per-client bandwidth limiting and quota enforcement.
#[allow(clippy::too_many_arguments)]
pub async fn relay_encrypted_with_limits<R, W>(
    tunnel_read: R,
    tunnel_write: W,
    outbound: TcpStream,
    cipher: Box<dyn AeadCipher>,
    session_keys: SessionKeys,
    metrics: Arc<ServerMetrics>,
    bytes_up: Arc<AtomicU64>,
    bytes_down: Arc<AtomicU64>,
    client_id: String,
    bandwidth: Arc<BandwidthLimiterStore>,
    quotas: Arc<QuotaStore>,
) -> Result<()>
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    // Check quota before starting relay
    if quotas.is_quota_exceeded(&client_id).await {
        return Err(anyhow::anyhow!(
            "Traffic quota exceeded for client {}",
            client_id
        ));
    }

    let (mut out_read, mut out_write) = outbound.into_split();
    let cipher: Arc<dyn AeadCipher> = Arc::from(cipher);
    let tunnel_write = Arc::new(Mutex::new(tunnel_write));
    let session_keys = Arc::new(Mutex::new(session_keys));

    let cipher_t2d = cipher.clone();
    let tunnel_write_ping = tunnel_write.clone();
    let session_keys_ping = session_keys.clone();
    let metrics_t2d = metrics.clone();
    let bytes_up_t2d = bytes_up.clone();
    let bw_up = bandwidth.clone();
    let q_up = quotas.clone();
    let cid_up = client_id.clone();

    // tunnel → destination (upload direction)
    let mut tunnel_read = tunnel_read;
    let tunnel_to_dest = tokio::spawn(async move {
        let mut anti_replay = AntiReplayWindow::new();
        let mut frame_buf = Vec::with_capacity(MAX_FRAME_SIZE);

        loop {
            let mut len_buf = [0u8; 2];
            if tunnel_read.read_exact(&mut len_buf).await.is_err() {
                break;
            }
            let frame_len = u16::from_be_bytes(len_buf) as usize;
            if frame_len > MAX_FRAME_SIZE {
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

            // Apply bandwidth limit (wait if rate-limited)
            bw_up.wait_upload(&cid_up, frame_bytes as u32).await;

            // Track quota
            if let Some(usage) = q_up.get(&cid_up).await {
                usage.add_upload(frame_bytes);
                if usage.quota_exceeded() {
                    warn!(client = %cid_up, "Upload quota exceeded mid-session");
                    break;
                }
            }

            bytes_up_t2d.fetch_add(frame_bytes, Ordering::Relaxed);
            metrics_t2d
                .total_bytes_up
                .fetch_add(frame_bytes, Ordering::Relaxed);

            match decrypt_frame(cipher_t2d.as_ref(), &frame_buf[..frame_len]) {
                Ok((plaintext, nonce)) => {
                    let counter = nonce_to_counter(&nonce);
                    if let Err(e) = anti_replay.check_and_update(counter) {
                        warn!("Anti-replay check failed: {}", e);
                        break;
                    }

                    match decode_data_frame(&plaintext) {
                        Ok(frame) => match frame.command {
                            Command::Data(data) => {
                                if out_write.write_all(&data).await.is_err() {
                                    break;
                                }
                            }
                            Command::Close => break,
                            Command::Ping(seq) => {
                                let pong = DataFrame {
                                    command: Command::Pong(seq),
                                    flags: 0,
                                    stream_id: frame.stream_id,
                                };
                                let pong_bytes = encode_data_frame(&pong);
                                let nonce = session_keys_ping.lock().await.next_server_nonce();
                                if let Ok(encrypted) =
                                    encrypt_frame(cipher_t2d.as_ref(), &nonce, &pong_bytes)
                                {
                                    let mut tw = tunnel_write_ping.lock().await;
                                    let len = (encrypted.len() as u16).to_be_bytes();
                                    let _ = tw.write_all(&len).await;
                                    let _ = tw.write_all(&encrypted).await;
                                }
                            }
                            _ => {}
                        },
                        Err(e) => {
                            warn!("Frame decode error: {}", e);
                            break;
                        }
                    }
                }
                Err(e) => {
                    warn!("Frame decrypt error: {}", e);
                    break;
                }
            }
        }
    });

    let cipher_d2t = cipher.clone();
    let session_keys_d2t = session_keys.clone();
    let session_keys_padding = session_keys.clone();

    // destination → tunnel (download direction)
    let dest_to_tunnel = tokio::spawn(async move {
        let sk = session_keys_padding.lock().await;
        let use_padding = sk.protocol_version >= PROTOCOL_VERSION_V2;
        let padding_range = sk.padding_range;
        drop(sk);

        let mut buf = vec![0u8; 8192];
        loop {
            match out_read.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    // Apply download bandwidth limit
                    bandwidth.wait_download(&client_id, n as u32).await;

                    let flags = if use_padding { FLAG_PADDED } else { 0 };
                    let frame = DataFrame {
                        command: Command::Data(buf[..n].to_vec()),
                        flags,
                        stream_id: 0,
                    };
                    let frame_bytes = if use_padding {
                        encode_data_frame_padded(&frame, &padding_range)
                    } else {
                        encode_data_frame(&frame)
                    };
                    let nonce = session_keys_d2t.lock().await.next_server_nonce();
                    match encrypt_frame(cipher_d2t.as_ref(), &nonce, &frame_bytes) {
                        Ok(encrypted) => {
                            let enc_len = encrypted.len() as u64 + 2;
                            bytes_down.fetch_add(enc_len, Ordering::Relaxed);
                            metrics
                                .total_bytes_down
                                .fetch_add(enc_len, Ordering::Relaxed);

                            // Track quota
                            if let Some(usage) = quotas.get(&client_id).await {
                                usage.add_download(enc_len);
                            }

                            let mut tw = tunnel_write.lock().await;
                            let len = (encrypted.len() as u16).to_be_bytes();
                            if tw.write_all(&len).await.is_err() {
                                break;
                            }
                            if tw.write_all(&encrypted).await.is_err() {
                                break;
                            }
                        }
                        Err(e) => {
                            warn!("Frame encrypt error: {}", e);
                            break;
                        }
                    }
                }
                Err(_) => break,
            }
        }
    });

    tokio::select! {
        _ = tunnel_to_dest => {},
        _ = dest_to_tunnel => {},
    }

    debug!("Rate-limited relay session ended");
    Ok(())
}

/// Extract the 8-byte counter from a 12-byte nonce.
/// Nonce format: [direction:1][0:3][counter:8]
fn nonce_to_counter(nonce: &[u8; 12]) -> u64 {
    u64::from_be_bytes(nonce[4..12].try_into().unwrap())
}
