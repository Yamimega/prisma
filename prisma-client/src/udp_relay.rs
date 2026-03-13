use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Result;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UdpSocket;
use tokio::sync::Mutex;
use tracing::{debug, trace, warn};

use prisma_core::crypto::aead::AeadCipher;
use prisma_core::fec::{decode_fec_header, encode_fec_header, FecConfig, FecDecoder, FecEncoder};
use prisma_core::protocol::codec::*;
use prisma_core::protocol::types::*;
use prisma_core::types::{MAX_FRAME_SIZE, PROTOCOL_VERSION_V2};

use crate::tunnel::TunnelConnection;

/// Run a UDP relay between a local UDP socket and an encrypted tunnel.
///
/// SOCKS5 UDP clients send datagrams (with SOCKS5 UDP header) to `local_socket`.
/// We strip the SOCKS5 header, wrap the payload in CMD_UDP_DATA, and send through the tunnel.
/// Responses come back as CMD_UDP_DATA frames and are relayed to the original peer
/// with the SOCKS5 UDP header prepended.
pub async fn relay_udp(
    local_socket: Arc<UdpSocket>,
    tunnel: TunnelConnection,
    fec_config: Option<FecConfig>,
) -> Result<()> {
    let (mut tunnel_read, mut tunnel_write) = tokio::io::split(tunnel.stream);
    let cipher: Arc<dyn AeadCipher> = Arc::from(tunnel.cipher);

    // Track SOCKS5 client addresses per association
    let peer_map: Arc<Mutex<HashMap<u32, SocketAddr>>> = Arc::new(Mutex::new(HashMap::new()));
    let local_socket_recv = local_socket.clone();

    // FEC encoder for send direction
    let fec_encoder: Option<Arc<Mutex<FecEncoder>>> = fec_config.as_ref().map(|cfg| {
        Arc::new(Mutex::new(FecEncoder::new(
            cfg.data_shards,
            cfg.parity_shards,
        )))
    });

    // FEC decoder for receive direction
    let fec_decoder: Option<Arc<Mutex<FecDecoder>>> = fec_config.as_ref().map(|cfg| {
        Arc::new(Mutex::new(FecDecoder::new(
            cfg.data_shards,
            cfg.parity_shards,
        )))
    });

    // Sequence counter for receive-side FEC data shard tracking
    let recv_seq: Arc<Mutex<u8>> = Arc::new(Mutex::new(0));

    // Local UDP → Tunnel: read SOCKS5 UDP datagrams, encrypt as CMD_UDP_DATA
    let mut client_keys = tunnel.session_keys.clone();
    let use_padding = client_keys.protocol_version >= PROTOCOL_VERSION_V2;
    let padding_range = client_keys.padding_range;
    let cipher_up = cipher.clone();
    let peer_map_up = peer_map.clone();
    let fec_encoder_up = fec_encoder.clone();
    let fec_config_up = fec_config.clone();

    let udp_to_tunnel = async move {
        let mut buf = vec![0u8; 65535];
        loop {
            let (n, peer) = match local_socket_recv.recv_from(&mut buf).await {
                Ok(r) => r,
                Err(e) => {
                    warn!("UDP recv error: {}", e);
                    break;
                }
            };

            if n < 4 {
                continue; // Too short for SOCKS5 UDP header
            }

            // Parse SOCKS5 UDP request header: [RSV:2][FRAG:1][ATYP:1][DST.ADDR:var][DST.PORT:2][DATA:var]
            let frag = buf[2];
            let atyp = buf[3];

            let (addr_type, dest_addr, dest_port, data_offset) = match atyp {
                0x01 => {
                    // IPv4
                    if n < 10 {
                        continue;
                    }
                    let addr = buf[4..8].to_vec();
                    let port = u16::from_be_bytes([buf[8], buf[9]]);
                    (0x01u8, addr, port, 10)
                }
                0x03 => {
                    // Domain
                    if n < 5 {
                        continue;
                    }
                    let domain_len = buf[4] as usize;
                    if n < 5 + domain_len + 2 {
                        continue;
                    }
                    let addr = buf[5..5 + domain_len].to_vec();
                    let port = u16::from_be_bytes([buf[5 + domain_len], buf[5 + domain_len + 1]]);
                    (0x03u8, addr, port, 5 + domain_len + 2)
                }
                0x04 => {
                    // IPv6
                    if n < 22 {
                        continue;
                    }
                    let addr = buf[4..20].to_vec();
                    let port = u16::from_be_bytes([buf[20], buf[21]]);
                    (0x04u8, addr, port, 22)
                }
                _ => continue,
            };

            let payload = buf[data_offset..n].to_vec();

            // Remember the peer for this association so we can send responses back
            let assoc_id = 1; // Single association per tunnel
            peer_map_up.lock().await.insert(assoc_id, peer);

            let flags = if use_padding { FLAG_PADDED } else { 0 };
            let frame = DataFrame {
                command: Command::UdpData {
                    assoc_id,
                    frag,
                    addr_type,
                    dest_addr: dest_addr.clone(),
                    dest_port,
                    payload: payload.clone(),
                },
                flags,
                stream_id: 0,
            };
            let frame_bytes = if use_padding {
                encode_data_frame_padded(&frame, &padding_range)
            } else {
                encode_data_frame(&frame)
            };
            let nonce = client_keys.next_client_nonce();
            match encrypt_frame(cipher_up.as_ref(), &nonce, &frame_bytes) {
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
                    warn!("UDP relay encrypt error: {}", e);
                    break;
                }
            }

            // FEC: feed the payload to the encoder and send parity shards when group completes
            if let (Some(ref encoder), Some(ref cfg)) = (&fec_encoder_up, &fec_config_up) {
                let mut enc = encoder.lock().await;
                if let Some(group) = enc.add_shard(&payload) {
                    let total = group.data_shards + group.parity_shards;
                    // Send only parity shards (data shards were already sent as regular frames)
                    for i in group.data_shards as usize
                        ..(group.data_shards + group.parity_shards) as usize
                    {
                        let header = encode_fec_header(group.group_id, i as u8, total);
                        let mut fec_payload = Vec::with_capacity(4 + group.shards[i].len());
                        fec_payload.extend_from_slice(&header);
                        fec_payload.extend_from_slice(&group.shards[i]);

                        let fec_flags = flags | FLAG_FEC;
                        let parity_frame = DataFrame {
                            command: Command::UdpData {
                                assoc_id,
                                frag: 0,
                                addr_type: dest_addr.first().copied().unwrap_or(0x01),
                                dest_addr: vec![],
                                dest_port: 0,
                                payload: fec_payload,
                            },
                            flags: fec_flags,
                            stream_id: 0,
                        };
                        let parity_bytes = if use_padding {
                            encode_data_frame_padded(&parity_frame, &padding_range)
                        } else {
                            encode_data_frame(&parity_frame)
                        };
                        let parity_nonce = client_keys.next_client_nonce();
                        match encrypt_frame(cipher_up.as_ref(), &parity_nonce, &parity_bytes) {
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
                                warn!("UDP relay FEC parity encrypt error: {}", e);
                                break;
                            }
                        }
                    }
                    let _ = cfg; // suppress unused warning
                }
            }
        }
    };

    // Tunnel → Local UDP: decrypt CMD_UDP_DATA frames, send to SOCKS5 client with header
    let cipher_down = cipher.clone();
    let tunnel_to_udp = async move {
        let mut frame_buf = Vec::with_capacity(MAX_FRAME_SIZE);
        loop {
            let mut len_buf = [0u8; 2];
            if tunnel_read.read_exact(&mut len_buf).await.is_err() {
                break;
            }
            let frame_len = u16::from_be_bytes(len_buf) as usize;
            if frame_len > MAX_FRAME_SIZE {
                warn!(size = frame_len, "UDP relay: frame too large from server");
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

            match decrypt_frame(cipher_down.as_ref(), &frame_buf[..frame_len]) {
                Ok((plaintext, _)) => match decode_data_frame(&plaintext) {
                    Ok(frame) => match frame.command {
                        Command::UdpData {
                            assoc_id,
                            frag,
                            addr_type,
                            dest_addr,
                            dest_port,
                            payload,
                        } => {
                            if frame.flags & FLAG_FEC != 0 {
                                // FEC parity shard: feed to decoder
                                if let Some(ref decoder) = fec_decoder {
                                    if payload.len() < 4 {
                                        warn!("FEC shard too short");
                                        continue;
                                    }
                                    let header: [u8; 4] = payload[..4].try_into().unwrap();
                                    let (group_id, shard_index, _total) =
                                        decode_fec_header(&header);
                                    let shard_data = &payload[4..];

                                    let mut dec = decoder.lock().await;
                                    if let Some(recovered_shards) =
                                        dec.add_shard(group_id, shard_index, shard_data)
                                    {
                                        // Deliver recovered data shards to the SOCKS5 client
                                        // Note: recovered shards are raw payloads; we can't
                                        // reconstruct the full SOCKS5 header per-shard since
                                        // address info isn't embedded. Recovery is best-effort.
                                        trace!(
                                            group_id,
                                            count = recovered_shards.len(),
                                            "FEC recovered shards"
                                        );
                                    }
                                }
                            } else {
                                // Regular data frame: deliver to SOCKS5 client
                                // Also feed to FEC decoder as a data shard
                                if let Some(ref decoder) = fec_decoder {
                                    let mut seq = recv_seq.lock().await;
                                    let shard_index = *seq;
                                    let fec_cfg = fec_config.as_ref().unwrap();
                                    let _fec_total =
                                        (fec_cfg.data_shards + fec_cfg.parity_shards) as u8;
                                    // Compute group_id from sequence
                                    let group_id =
                                        (shard_index as u16) / (fec_cfg.data_shards as u16);
                                    let index_in_group = shard_index % (fec_cfg.data_shards as u8);

                                    let mut dec = decoder.lock().await;
                                    let _ = dec.add_shard(group_id, index_in_group, &payload);
                                    *seq = seq.wrapping_add(1);
                                }

                                let peer = peer_map.lock().await.get(&assoc_id).cloned();
                                if let Some(peer) = peer {
                                    let header = build_socks5_udp_header(
                                        frag, addr_type, &dest_addr, dest_port,
                                    );
                                    let mut response = header;
                                    response.extend_from_slice(&payload);
                                    if let Err(e) = local_socket.send_to(&response, peer).await {
                                        warn!("UDP send to SOCKS5 client failed: {}", e);
                                    }
                                }
                            }
                        }
                        Command::ForwardReady { .. } => {
                            // Acknowledgment of UdpAssociate, already handled
                            debug!("Received ForwardReady for UDP association");
                        }
                        Command::Close => break,
                        _ => {}
                    },
                    Err(e) => {
                        warn!("UDP relay frame decode error: {}", e);
                        break;
                    }
                },
                Err(e) => {
                    warn!("UDP relay decrypt error: {}", e);
                    break;
                }
            }
        }
    };

    tokio::select! {
        _ = udp_to_tunnel => {},
        _ = tunnel_to_udp => {},
    }

    debug!("UDP relay session ended");
    Ok(())
}

/// Build a SOCKS5 UDP response header: [RSV:2][FRAG:1][ATYP:1][DST.ADDR:var][DST.PORT:2]
fn build_socks5_udp_header(frag: u8, addr_type: u8, addr: &[u8], port: u16) -> Vec<u8> {
    let mut header = Vec::with_capacity(4 + addr.len() + 2);
    header.extend_from_slice(&[0x00, 0x00]); // RSV
    header.push(frag); // FRAG
    header.push(addr_type); // ATYP

    if addr_type == 0x03 {
        // Domain: prepend length byte
        header.push(addr.len() as u8);
    }
    header.extend_from_slice(addr);
    header.extend_from_slice(&port.to_be_bytes());
    header
}
