use anyhow::Result;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tracing::{debug, warn};

use prisma_core::protocol::frame_encoder::{FrameDecoder, FrameEncoder, MAX_PAYLOAD_SIZE};
use prisma_core::protocol::types::*;
use prisma_core::types::MAX_FRAME_SIZE;

use crate::tunnel::TunnelConnection;

/// Bidirectional relay between SOCKS5 client and encrypted tunnel.
///
/// SOCKS5 client ↔ [plain TCP] ↔ relay ↔ [encrypted frames] ↔ tunnel
///
/// Performance optimizations:
/// - 32KB read buffer (4x larger, reduces frame count for bulk transfers)
/// - FrameEncoder with zero-copy in-place encryption (no heap allocations)
/// - FrameDecoder with in-place decryption
/// - Write coalescing (single syscall per frame)
pub async fn relay(socks_stream: TcpStream, tunnel: TunnelConnection) -> Result<()> {
    let (mut socks_read, mut socks_write) = socks_stream.into_split();
    let (mut tunnel_read, mut tunnel_write) = tokio::io::split(tunnel.stream);

    let cipher: Arc<dyn prisma_core::crypto::aead::AeadCipher> = Arc::from(tunnel.cipher);

    // SOCKS5 → tunnel: read raw data, encrypt into frames
    let mut client_keys = tunnel.session_keys.clone();
    let padding_range = client_keys.padding_range;
    let cipher_s2t = cipher.clone();
    let socks_to_tunnel = async move {
        let mut encoder = FrameEncoder::new();
        let mut buf = vec![0u8; MAX_PAYLOAD_SIZE];
        loop {
            match socks_read.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    let nonce = client_keys.next_client_nonce();

                    // Copy payload into encoder buffer and seal in-place
                    encoder.payload_mut()[..n].copy_from_slice(&buf[..n]);
                    match encoder.seal_data_frame(cipher_s2t.as_ref(), &nonce, n, 0, &padding_range)
                    {
                        Ok(wire) => {
                            // Single write_all call (coalesced: outer_len + nonce + data + tag)
                            if tunnel_write.write_all(wire).await.is_err() {
                                break;
                            }
                        }
                        Err(e) => {
                            warn!("Encrypt error: {}", e);
                            break;
                        }
                    }
                }
                Err(_) => break,
            }
        }
    };

    // tunnel → SOCKS5: decrypt frames, send raw data
    let tunnel_to_socks = async move {
        let mut frame_buf = vec![0u8; MAX_FRAME_SIZE];
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
            if tunnel_read
                .read_exact(&mut frame_buf[..frame_len])
                .await
                .is_err()
            {
                break;
            }

            // Decrypt in-place using FrameDecoder
            match FrameDecoder::unseal_data_frame(
                &mut frame_buf[..frame_len],
                frame_len,
                cipher.as_ref(),
            ) {
                Ok((cmd, payload, _nonce)) => match cmd {
                    CMD_DATA => {
                        if socks_write.write_all(payload).await.is_err() {
                            break;
                        }
                    }
                    CMD_CLOSE => break,
                    _ => {}
                },
                Err(e) => {
                    warn!("Decrypt error: {}", e);
                    break;
                }
            }
        }
    };

    tokio::select! {
        _ = socks_to_tunnel => {},
        _ = tunnel_to_socks => {},
    }

    debug!("Relay session ended");
    Ok(())
}

/// Relay data between a smoltcp TCP socket (via TUN) and an encrypted PrismaVeil tunnel.
///
/// Reads data from the smoltcp socket, encrypts it, and sends through the tunnel.
/// Reads encrypted data from the tunnel, decrypts it, and writes to the smoltcp socket.
pub async fn relay_tun_tcp_encrypted<R, W>(
    handle: smoltcp::iface::SocketHandle,
    stack: Arc<tokio::sync::Mutex<crate::tun::tcp_stack::TcpStack>>,
    mut tunnel_read: R,
    mut tunnel_write: W,
    cipher: Box<dyn prisma_core::crypto::aead::AeadCipher>,
    mut session_keys: prisma_core::protocol::types::SessionKeys,
) -> Result<()>
where
    R: tokio::io::AsyncRead + Unpin + Send,
    W: tokio::io::AsyncWrite + Unpin + Send,
{
    let cipher: Arc<dyn prisma_core::crypto::aead::AeadCipher> = Arc::from(cipher);
    let padding_range = session_keys.padding_range;

    // Poll interval for checking smoltcp socket state
    let mut interval = tokio::time::interval(std::time::Duration::from_millis(5));

    let mut encoder = FrameEncoder::new();
    let mut local_buf = vec![0u8; MAX_PAYLOAD_SIZE];
    let mut frame_buf = vec![0u8; MAX_FRAME_SIZE];

    loop {
        tokio::select! {
            _ = interval.tick() => {
                // Read data and check close state in a single lock acquisition
                let (n, is_closed) = {
                    let mut s = stack.lock().await;
                    let n = s.read_from_socket(handle, &mut local_buf);
                    let closed = s.is_closed(handle);
                    (n, closed)
                };
                if n > 0 {
                    let nonce = session_keys.next_client_nonce();
                    encoder.payload_mut()[..n].copy_from_slice(&local_buf[..n]);
                    match encoder.seal_data_frame(
                        cipher.as_ref(),
                        &nonce,
                        n,
                        0,
                        &padding_range,
                    ) {
                        Ok(wire) => {
                            if tunnel_write.write_all(wire).await.is_err() { break; }
                        }
                        Err(e) => {
                            warn!("TUN relay encrypt error: {}", e);
                            break;
                        }
                    }
                }
                if is_closed { break; }
            }
            // Read encrypted data from tunnel → decrypt → write to smoltcp socket
            result = async {
                let mut len_buf = [0u8; 2];
                tunnel_read.read_exact(&mut len_buf).await?;
                let frame_len = u16::from_be_bytes(len_buf) as usize;
                if frame_len > MAX_FRAME_SIZE {
                    return Err(anyhow::anyhow!("Frame too large"));
                }
                tunnel_read.read_exact(&mut frame_buf[..frame_len]).await?;
                Ok::<_, anyhow::Error>(frame_len)
            } => {
                match result {
                    Ok(frame_len) => {
                        match FrameDecoder::unseal_data_frame(
                            &mut frame_buf[..frame_len],
                            frame_len,
                            cipher.as_ref(),
                        ) {
                            Ok((cmd, payload, _nonce)) => {
                                match cmd {
                                    CMD_DATA => {
                                        let mut s = stack.lock().await;
                                        s.write_to_socket(handle, payload);
                                    }
                                    CMD_CLOSE => break,
                                    _ => {}
                                }
                            }
                            Err(e) => {
                                warn!("TUN relay decrypt error: {}", e);
                                break;
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
        }
    }

    debug!("TUN TCP relay ended");
    Ok(())
}

/// Direct relay between local client and outbound connection (no encryption).
/// Used when routing rules select "direct" action.
pub async fn relay_direct(local: TcpStream, outbound: TcpStream) -> Result<()> {
    let (mut local_read, mut local_write) = local.into_split();
    let (mut out_read, mut out_write) = outbound.into_split();

    let l2o = async move {
        let _ = tokio::io::copy(&mut local_read, &mut out_write).await;
    };
    let o2l = async move {
        let _ = tokio::io::copy(&mut out_read, &mut local_write).await;
    };

    tokio::select! {
        _ = l2o => {},
        _ = o2l => {},
    }

    debug!("Direct relay session ended");
    Ok(())
}
