use anyhow::Result;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tracing::{debug, warn};

use prisma_core::protocol::codec::*;
use prisma_core::protocol::types::*;
use prisma_core::types::MAX_FRAME_SIZE;

use crate::tunnel::TunnelConnection;

/// Bidirectional relay between SOCKS5 client and encrypted tunnel.
///
/// SOCKS5 client ↔ [plain TCP] ↔ relay ↔ [encrypted frames] ↔ tunnel
pub async fn relay(socks_stream: TcpStream, tunnel: TunnelConnection) -> Result<()> {
    let (mut socks_read, mut socks_write) = socks_stream.into_split();
    let (mut tunnel_read, mut tunnel_write) = tokio::io::split(tunnel.stream);

    let cipher: Arc<dyn prisma_core::crypto::aead::AeadCipher> = Arc::from(tunnel.cipher);

    // SOCKS5 → tunnel: read raw data, encrypt into frames
    let mut client_keys = tunnel.session_keys.clone();
    let cipher_s2t = cipher.clone();
    let socks_to_tunnel = async move {
        let mut buf = vec![0u8; 8192];
        loop {
            match socks_read.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    let frame = DataFrame {
                        command: Command::Data(buf[..n].to_vec()),
                        flags: 0,
                        stream_id: 0,
                    };
                    let frame_bytes = encode_data_frame(&frame);
                    let nonce = client_keys.next_client_nonce();
                    match encrypt_frame(cipher_s2t.as_ref(), &nonce, &frame_bytes) {
                        Ok(encrypted) => {
                            let len = (encrypted.len() as u16).to_be_bytes();
                            if tunnel_write.write_all(&len).await.is_err() {
                                break;
                            }
                            if tunnel_write.write_all(&encrypted).await.is_err() {
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
    let cipher_t2s = cipher.clone();
    let tunnel_to_socks = async move {
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
            if tunnel_read.read_exact(&mut frame_buf[..frame_len]).await.is_err() {
                break;
            }

            match decrypt_frame(cipher_t2s.as_ref(), &frame_buf[..frame_len]) {
                Ok((plaintext, _)) => match decode_data_frame(&plaintext) {
                    Ok(frame) => match frame.command {
                        Command::Data(data) => {
                            if socks_write.write_all(&data).await.is_err() {
                                break;
                            }
                        }
                        Command::Close => break,
                        _ => {}
                    },
                    Err(e) => {
                        warn!("Frame decode error: {}", e);
                        break;
                    }
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
