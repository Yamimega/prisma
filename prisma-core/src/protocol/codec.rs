use std::net::{Ipv4Addr, Ipv6Addr};

use crate::error::{CryptoError, ProtocolError};
use crate::types::{CipherSuite, ClientId, ProxyAddress, ProxyDestination, NONCE_SIZE};

use super::types::*;

// --- Handshake message encoding/decoding (v4 only) ---

/// Encode PrismaClientInit to bytes.
/// Wire format:
///   [version:1][flags:1][client_ephemeral_pub:32][client_id:16][timestamp:8]
///   [cipher_suite:1][auth_token:32][padding:var]
pub fn encode_client_init(msg: &PrismaClientInit) -> Vec<u8> {
    let mut buf = Vec::with_capacity(1 + 1 + 32 + 16 + 8 + 1 + 32 + msg.padding.len());
    buf.push(msg.version);
    buf.push(msg.flags);
    buf.extend_from_slice(&msg.client_ephemeral_pub);
    buf.extend_from_slice(msg.client_id.0.as_bytes());
    buf.extend_from_slice(&msg.timestamp.to_be_bytes());
    buf.push(msg.cipher_suite as u8);
    buf.extend_from_slice(&msg.auth_token);
    buf.extend_from_slice(&msg.padding);
    buf
}

/// Decode PrismaClientInit from bytes.
pub fn decode_client_init(data: &[u8]) -> Result<PrismaClientInit, ProtocolError> {
    // Minimum: 1+1+32+16+8+1+32 = 91
    if data.len() < 91 {
        return Err(ProtocolError::InvalidFrame(
            "PrismaClientInit too short".to_string(),
        ));
    }
    let version = data[0];
    let flags = data[1];
    let mut client_ephemeral_pub = [0u8; 32];
    client_ephemeral_pub.copy_from_slice(&data[2..34]);
    let client_id = ClientId(uuid::Uuid::from_bytes(data[34..50].try_into().unwrap()));
    let timestamp = u64::from_be_bytes(data[50..58].try_into().unwrap());
    let cipher_suite =
        CipherSuite::from_u8(data[58]).ok_or(ProtocolError::InvalidCommand(data[58]))?;
    let mut auth_token = [0u8; 32];
    auth_token.copy_from_slice(&data[59..91]);
    let padding = data[91..].to_vec();

    Ok(PrismaClientInit {
        version,
        flags,
        client_ephemeral_pub,
        client_id,
        timestamp,
        cipher_suite,
        auth_token,
        padding,
    })
}

/// Encode PrismaServerInit to bytes (plaintext — will be encrypted with preliminary key).
/// Wire format:
///   [status:1][session_id:16][server_ephemeral_pub:32][challenge:32]
///   [padding_min:2][padding_max:2][server_features:4]
///   [ticket_len:2][ticket:var]
///   [bucket_count:2][bucket_sizes:2*N][padding:var]
pub fn encode_server_init(msg: &PrismaServerInit) -> Vec<u8> {
    let bucket_bytes = msg.bucket_sizes.len() * 2;
    let mut buf = Vec::with_capacity(
        1 + 16
            + 32
            + 32
            + 2
            + 2
            + 4
            + 2
            + msg.session_ticket.len()
            + 2
            + bucket_bytes
            + msg.padding.len(),
    );
    buf.push(msg.status as u8);
    buf.extend_from_slice(msg.session_id.as_bytes());
    buf.extend_from_slice(&msg.server_ephemeral_pub);
    buf.extend_from_slice(&msg.challenge);
    buf.extend_from_slice(&msg.padding_min.to_le_bytes());
    buf.extend_from_slice(&msg.padding_max.to_le_bytes());
    buf.extend_from_slice(&msg.server_features.to_le_bytes());
    buf.extend_from_slice(&(msg.session_ticket.len() as u16).to_be_bytes());
    buf.extend_from_slice(&msg.session_ticket);
    buf.extend_from_slice(&(msg.bucket_sizes.len() as u16).to_be_bytes());
    for &size in &msg.bucket_sizes {
        buf.extend_from_slice(&size.to_le_bytes());
    }
    buf.extend_from_slice(&msg.padding);
    buf
}

/// Decode PrismaServerInit from bytes.
pub fn decode_server_init(data: &[u8]) -> Result<PrismaServerInit, ProtocolError> {
    // Minimum: 1+16+32+32+2+2+4+2+0+2 = 93
    if data.len() < 93 {
        return Err(ProtocolError::InvalidFrame(
            "PrismaServerInit too short".to_string(),
        ));
    }
    let status = AcceptStatus::from_u8(data[0]).ok_or(ProtocolError::InvalidFrame(
        "Invalid PrismaServerInit status".to_string(),
    ))?;
    let session_id = uuid::Uuid::from_bytes(data[1..17].try_into().unwrap());
    let mut server_ephemeral_pub = [0u8; 32];
    server_ephemeral_pub.copy_from_slice(&data[17..49]);
    let mut challenge = [0u8; 32];
    challenge.copy_from_slice(&data[49..81]);
    let padding_min = u16::from_le_bytes([data[81], data[82]]);
    let padding_max = u16::from_le_bytes([data[83], data[84]]);
    let server_features = u32::from_le_bytes([data[85], data[86], data[87], data[88]]);
    let ticket_len = u16::from_be_bytes([data[89], data[90]]) as usize;
    let ticket_end = 91 + ticket_len;
    if data.len() < ticket_end + 2 {
        return Err(ProtocolError::InvalidFrame(
            "PrismaServerInit ticket/buckets truncated".to_string(),
        ));
    }
    let session_ticket = data[91..ticket_end].to_vec();
    let bucket_count = u16::from_be_bytes([data[ticket_end], data[ticket_end + 1]]) as usize;
    let buckets_start = ticket_end + 2;
    let buckets_end = buckets_start + bucket_count * 2;
    if data.len() < buckets_end {
        return Err(ProtocolError::InvalidFrame(
            "PrismaServerInit bucket sizes truncated".to_string(),
        ));
    }
    let mut bucket_sizes = Vec::with_capacity(bucket_count);
    for i in 0..bucket_count {
        let offset = buckets_start + i * 2;
        bucket_sizes.push(u16::from_le_bytes([data[offset], data[offset + 1]]));
    }
    let padding = data[buckets_end..].to_vec();

    Ok(PrismaServerInit {
        status,
        session_id,
        server_ephemeral_pub,
        challenge,
        padding_min,
        padding_max,
        server_features,
        session_ticket,
        bucket_sizes,
        padding,
    })
}

/// Encode PrismaClientResume to bytes (0-RTT).
pub fn encode_client_resume(msg: &PrismaClientResume) -> Vec<u8> {
    let mut buf = Vec::with_capacity(
        1 + 1 + 32 + 2 + msg.session_ticket.len() + msg.encrypted_0rtt_data.len(),
    );
    buf.push(msg.version);
    buf.push(msg.flags);
    buf.extend_from_slice(&msg.client_ephemeral_pub);
    buf.extend_from_slice(&(msg.session_ticket.len() as u16).to_be_bytes());
    buf.extend_from_slice(&msg.session_ticket);
    buf.extend_from_slice(&msg.encrypted_0rtt_data);
    buf
}

/// Decode PrismaClientResume from bytes (0-RTT).
pub fn decode_client_resume(data: &[u8]) -> Result<PrismaClientResume, ProtocolError> {
    // Minimum: 1+1+32+2 = 36
    if data.len() < 36 {
        return Err(ProtocolError::InvalidFrame(
            "PrismaClientResume too short".to_string(),
        ));
    }
    let version = data[0];
    let flags = data[1];
    let mut client_ephemeral_pub = [0u8; 32];
    client_ephemeral_pub.copy_from_slice(&data[2..34]);
    let ticket_len = u16::from_be_bytes([data[34], data[35]]) as usize;
    if data.len() < 36 + ticket_len {
        return Err(ProtocolError::InvalidFrame(
            "PrismaClientResume ticket truncated".to_string(),
        ));
    }
    let session_ticket = data[36..36 + ticket_len].to_vec();
    let encrypted_0rtt_data = data[36 + ticket_len..].to_vec();

    Ok(PrismaClientResume {
        version,
        flags,
        client_ephemeral_pub,
        session_ticket,
        encrypted_0rtt_data,
    })
}

/// Encode SessionTicket to plaintext bytes (server encrypts before sending).
pub fn encode_session_ticket(ticket: &SessionTicket) -> Vec<u8> {
    let mut buf = Vec::with_capacity(16 + 32 + 1 + 8 + 2 + 2);
    buf.extend_from_slice(ticket.client_id.0.as_bytes());
    buf.extend_from_slice(&ticket.session_key);
    buf.push(ticket.cipher_suite as u8);
    buf.extend_from_slice(&ticket.issued_at.to_be_bytes());
    buf.extend_from_slice(&ticket.padding_range.min.to_be_bytes());
    buf.extend_from_slice(&ticket.padding_range.max.to_be_bytes());
    buf
}

/// Decode SessionTicket from plaintext bytes.
pub fn decode_session_ticket(data: &[u8]) -> Result<SessionTicket, ProtocolError> {
    // 16+32+1+8+2+2 = 61
    if data.len() < 61 {
        return Err(ProtocolError::InvalidFrame(
            "SessionTicket too short".to_string(),
        ));
    }
    let client_id = ClientId(uuid::Uuid::from_bytes(data[..16].try_into().unwrap()));
    let mut session_key = [0u8; 32];
    session_key.copy_from_slice(&data[16..48]);
    let cipher_suite =
        CipherSuite::from_u8(data[48]).ok_or(ProtocolError::InvalidCommand(data[48]))?;
    let issued_at = u64::from_be_bytes(data[49..57].try_into().unwrap());
    let padding_min = u16::from_be_bytes([data[57], data[58]]);
    let padding_max = u16::from_be_bytes([data[59], data[60]]);

    Ok(SessionTicket {
        client_id,
        session_key,
        cipher_suite,
        issued_at,
        padding_range: crate::types::PaddingRange::new(padding_min, padding_max),
    })
}

// --- DataFrame encoding/decoding ---

/// Encode a DataFrame to plaintext bytes.
/// v3 format: [cmd:1][flags:2 LE][stream_id:4][payload:var]
/// v2 padded: [cmd:1][flags:2 LE][stream_id:4][payload_len:2][payload:var][padding:var]
pub fn encode_data_frame(frame: &DataFrame) -> Vec<u8> {
    let payload = encode_command_payload(&frame.command);
    if frame.flags & FLAG_PADDED != 0 {
        // Padded format: include payload_len so receiver can split payload from padding
        let payload_len = payload.len() as u16;
        let mut buf = Vec::with_capacity(7 + 2 + payload.len());
        buf.push(frame.command.cmd_byte());
        buf.extend_from_slice(&frame.flags.to_le_bytes());
        buf.extend_from_slice(&frame.stream_id.to_be_bytes());
        buf.extend_from_slice(&payload_len.to_be_bytes());
        buf.extend_from_slice(&payload);
        // Padding is appended by the caller after encoding (via encode_data_frame_padded)
        buf
    } else {
        let mut buf = Vec::with_capacity(7 + payload.len());
        buf.push(frame.command.cmd_byte());
        buf.extend_from_slice(&frame.flags.to_le_bytes());
        buf.extend_from_slice(&frame.stream_id.to_be_bytes());
        buf.extend_from_slice(&payload);
        buf
    }
}

/// Encode a DataFrame with padding appended.
pub fn encode_data_frame_padded(
    frame: &DataFrame,
    padding_range: &crate::types::PaddingRange,
) -> Vec<u8> {
    let mut buf = encode_data_frame(frame);
    if frame.flags & FLAG_PADDED != 0 {
        let padding = crate::crypto::padding::generate_frame_padding(padding_range);
        buf.extend_from_slice(&padding);
    }
    buf
}

/// Decode a DataFrame from plaintext bytes.
/// v3 format: [cmd:1][flags:2 LE][stream_id:4][payload:var]
pub fn decode_data_frame(data: &[u8]) -> Result<DataFrame, ProtocolError> {
    if data.len() < 7 {
        return Err(ProtocolError::InvalidFrame(
            "DataFrame too short".to_string(),
        ));
    }
    let cmd = data[0];
    let flags = u16::from_le_bytes([data[1], data[2]]);
    let stream_id = u32::from_be_bytes(data[3..7].try_into().unwrap());

    let payload = if flags & FLAG_PADDED != 0 {
        // Padded format: [payload_len:2][payload:var][padding:var]
        if data.len() < 9 {
            return Err(ProtocolError::InvalidFrame(
                "Padded DataFrame too short for payload_len".to_string(),
            ));
        }
        let payload_len = u16::from_be_bytes([data[7], data[8]]) as usize;
        if data.len() < 9 + payload_len {
            return Err(ProtocolError::InvalidFrame(
                "Padded DataFrame payload truncated".to_string(),
            ));
        }
        // Strip padding — only return the actual payload
        &data[9..9 + payload_len]
    } else {
        &data[7..]
    };

    let command = decode_command_payload(cmd, payload)?;

    Ok(DataFrame {
        command,
        flags,
        stream_id,
    })
}

pub fn encode_command_payload(cmd: &Command) -> Vec<u8> {
    match cmd {
        Command::Connect(dest) => encode_proxy_destination(dest),
        Command::Data(data) => data.clone(),
        Command::Close => Vec::new(),
        Command::Ping(seq) => seq.to_be_bytes().to_vec(),
        Command::Pong(seq) => seq.to_be_bytes().to_vec(),
        Command::RegisterForward { remote_port, name } => {
            let name_bytes = name.as_bytes();
            let mut buf = Vec::with_capacity(2 + 1 + name_bytes.len());
            buf.extend_from_slice(&remote_port.to_be_bytes());
            buf.push(name_bytes.len() as u8);
            buf.extend_from_slice(name_bytes);
            buf
        }
        Command::ForwardReady {
            remote_port,
            success,
        } => {
            vec![
                (remote_port >> 8) as u8,
                *remote_port as u8,
                u8::from(*success),
            ]
        }
        Command::ForwardConnect { remote_port } => remote_port.to_be_bytes().to_vec(),
        // v3 commands
        Command::UdpAssociate {
            bind_addr_type,
            bind_addr,
            bind_port,
        } => {
            let mut buf = Vec::with_capacity(1 + bind_addr.len() + 2);
            buf.push(*bind_addr_type);
            buf.extend_from_slice(bind_addr);
            buf.extend_from_slice(&bind_port.to_be_bytes());
            buf
        }
        Command::UdpData {
            assoc_id,
            frag,
            addr_type,
            dest_addr,
            dest_port,
            payload,
        } => {
            let mut buf = Vec::with_capacity(4 + 1 + 1 + dest_addr.len() + 2 + payload.len());
            buf.extend_from_slice(&assoc_id.to_be_bytes());
            buf.push(*frag);
            buf.push(*addr_type);
            buf.extend_from_slice(dest_addr);
            buf.extend_from_slice(&dest_port.to_be_bytes());
            buf.extend_from_slice(payload);
            buf
        }
        Command::SpeedTest {
            direction,
            duration_secs,
            data,
        } => {
            let mut buf = Vec::with_capacity(2 + data.len());
            buf.push(*direction);
            buf.push(*duration_secs);
            buf.extend_from_slice(data);
            buf
        }
        Command::DnsQuery { query_id, data } => {
            let mut buf = Vec::with_capacity(2 + data.len());
            buf.extend_from_slice(&query_id.to_be_bytes());
            buf.extend_from_slice(data);
            buf
        }
        Command::DnsResponse { query_id, data } => {
            let mut buf = Vec::with_capacity(2 + data.len());
            buf.extend_from_slice(&query_id.to_be_bytes());
            buf.extend_from_slice(data);
            buf
        }
        Command::ChallengeResponse { hash } => hash.to_vec(),
    }
}

fn decode_command_payload(cmd: u8, payload: &[u8]) -> Result<Command, ProtocolError> {
    match cmd {
        CMD_CONNECT => {
            let dest = decode_proxy_destination(payload)?;
            Ok(Command::Connect(dest))
        }
        CMD_DATA => Ok(Command::Data(payload.to_vec())),
        CMD_CLOSE => Ok(Command::Close),
        CMD_PING => {
            if payload.len() < 4 {
                return Err(ProtocolError::InvalidFrame("Ping payload too short".into()));
            }
            Ok(Command::Ping(u32::from_be_bytes(
                payload[..4].try_into().unwrap(),
            )))
        }
        CMD_PONG => {
            if payload.len() < 4 {
                return Err(ProtocolError::InvalidFrame("Pong payload too short".into()));
            }
            Ok(Command::Pong(u32::from_be_bytes(
                payload[..4].try_into().unwrap(),
            )))
        }
        CMD_REGISTER_FORWARD => {
            if payload.len() < 3 {
                return Err(ProtocolError::InvalidFrame(
                    "RegisterForward too short".into(),
                ));
            }
            let remote_port = u16::from_be_bytes([payload[0], payload[1]]);
            let name_len = payload[2] as usize;
            if payload.len() < 3 + name_len {
                return Err(ProtocolError::InvalidFrame(
                    "RegisterForward name truncated".into(),
                ));
            }
            let name = String::from_utf8(payload[3..3 + name_len].to_vec())
                .map_err(|_| ProtocolError::InvalidFrame("Invalid forward name".into()))?;
            Ok(Command::RegisterForward { remote_port, name })
        }
        CMD_FORWARD_READY => {
            if payload.len() < 3 {
                return Err(ProtocolError::InvalidFrame("ForwardReady too short".into()));
            }
            let remote_port = u16::from_be_bytes([payload[0], payload[1]]);
            let success = payload[2] != 0;
            Ok(Command::ForwardReady {
                remote_port,
                success,
            })
        }
        CMD_FORWARD_CONNECT => {
            if payload.len() < 2 {
                return Err(ProtocolError::InvalidFrame(
                    "ForwardConnect too short".into(),
                ));
            }
            let remote_port = u16::from_be_bytes([payload[0], payload[1]]);
            Ok(Command::ForwardConnect { remote_port })
        }
        // v3 commands
        CMD_UDP_ASSOCIATE => {
            if payload.len() < 3 {
                return Err(ProtocolError::InvalidFrame("UdpAssociate too short".into()));
            }
            let bind_addr_type = payload[0];
            let bind_port =
                u16::from_be_bytes([payload[payload.len() - 2], payload[payload.len() - 1]]);
            let bind_addr = payload[1..payload.len() - 2].to_vec();
            Ok(Command::UdpAssociate {
                bind_addr_type,
                bind_addr,
                bind_port,
            })
        }
        CMD_UDP_DATA => {
            // [assoc_id:4][frag:1][addr_type:1][dest_addr:var][dest_port:2][payload:var]
            if payload.len() < 8 {
                return Err(ProtocolError::InvalidFrame("UdpData too short".into()));
            }
            let assoc_id = u32::from_be_bytes(payload[..4].try_into().unwrap());
            let frag = payload[4];
            let addr_type = payload[5];
            // Parse variable-length address based on type
            let (addr_end, dest_addr) = match addr_type {
                0x01 => {
                    // IPv4: 4 bytes
                    if payload.len() < 12 {
                        return Err(ProtocolError::InvalidFrame("UdpData IPv4 too short".into()));
                    }
                    (10, payload[6..10].to_vec())
                }
                0x04 => {
                    // IPv6: 16 bytes
                    if payload.len() < 24 {
                        return Err(ProtocolError::InvalidFrame("UdpData IPv6 too short".into()));
                    }
                    (22, payload[6..22].to_vec())
                }
                0x03 => {
                    // Domain: [len:1][domain:var]
                    if payload.len() < 7 {
                        return Err(ProtocolError::InvalidFrame(
                            "UdpData domain too short".into(),
                        ));
                    }
                    let domain_len = payload[6] as usize;
                    let end = 7 + domain_len;
                    if payload.len() < end + 2 {
                        return Err(ProtocolError::InvalidFrame(
                            "UdpData domain truncated".into(),
                        ));
                    }
                    (end, payload[6..end].to_vec())
                }
                _ => {
                    return Err(ProtocolError::InvalidAddressType(addr_type));
                }
            };
            if payload.len() < addr_end + 2 {
                return Err(ProtocolError::InvalidFrame(
                    "UdpData dest_port truncated".into(),
                ));
            }
            let dest_port = u16::from_be_bytes([payload[addr_end], payload[addr_end + 1]]);
            let udp_payload = payload[addr_end + 2..].to_vec();
            Ok(Command::UdpData {
                assoc_id,
                frag,
                addr_type,
                dest_addr,
                dest_port,
                payload: udp_payload,
            })
        }
        CMD_SPEED_TEST => {
            if payload.len() < 2 {
                return Err(ProtocolError::InvalidFrame("SpeedTest too short".into()));
            }
            Ok(Command::SpeedTest {
                direction: payload[0],
                duration_secs: payload[1],
                data: payload[2..].to_vec(),
            })
        }
        CMD_DNS_QUERY => {
            if payload.len() < 2 {
                return Err(ProtocolError::InvalidFrame("DnsQuery too short".into()));
            }
            let query_id = u16::from_be_bytes([payload[0], payload[1]]);
            Ok(Command::DnsQuery {
                query_id,
                data: payload[2..].to_vec(),
            })
        }
        CMD_DNS_RESPONSE => {
            if payload.len() < 2 {
                return Err(ProtocolError::InvalidFrame("DnsResponse too short".into()));
            }
            let query_id = u16::from_be_bytes([payload[0], payload[1]]);
            Ok(Command::DnsResponse {
                query_id,
                data: payload[2..].to_vec(),
            })
        }
        CMD_CHALLENGE_RESP => {
            if payload.len() < 32 {
                return Err(ProtocolError::InvalidFrame(
                    "ChallengeResponse too short".into(),
                ));
            }
            let mut hash = [0u8; 32];
            hash.copy_from_slice(&payload[..32]);
            Ok(Command::ChallengeResponse { hash })
        }
        _ => Err(ProtocolError::InvalidCommand(cmd)),
    }
}

/// Encode: [addr_type:1][address:var][port:2]
fn encode_proxy_destination(dest: &ProxyDestination) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.push(dest.address.addr_type());
    match &dest.address {
        ProxyAddress::Ipv4(addr) => buf.extend_from_slice(&addr.octets()),
        ProxyAddress::Ipv6(addr) => buf.extend_from_slice(&addr.octets()),
        ProxyAddress::Domain(domain) => {
            let bytes = domain.as_bytes();
            buf.push(bytes.len() as u8);
            buf.extend_from_slice(bytes);
        }
    }
    buf.extend_from_slice(&dest.port.to_be_bytes());
    buf
}

/// Decode: [addr_type:1][address:var][port:2]
fn decode_proxy_destination(data: &[u8]) -> Result<ProxyDestination, ProtocolError> {
    if data.is_empty() {
        return Err(ProtocolError::InvalidFrame("Empty destination".into()));
    }
    let addr_type = data[0];
    match addr_type {
        0x01 => {
            if data.len() < 7 {
                // 1 + 4 + 2
                return Err(ProtocolError::InvalidFrame("IPv4 dest too short".into()));
            }
            let addr = Ipv4Addr::new(data[1], data[2], data[3], data[4]);
            let port = u16::from_be_bytes([data[5], data[6]]);
            Ok(ProxyDestination {
                address: ProxyAddress::Ipv4(addr),
                port,
            })
        }
        0x03 => {
            if data.len() < 2 {
                return Err(ProtocolError::InvalidFrame("Domain dest too short".into()));
            }
            let len = data[1] as usize;
            if data.len() < 2 + len + 2 {
                return Err(ProtocolError::InvalidFrame("Domain dest truncated".into()));
            }
            let domain = String::from_utf8(data[2..2 + len].to_vec())
                .map_err(|_| ProtocolError::InvalidFrame("Invalid domain encoding".into()))?;
            let port = u16::from_be_bytes([data[2 + len], data[3 + len]]);
            Ok(ProxyDestination {
                address: ProxyAddress::Domain(domain),
                port,
            })
        }
        0x04 => {
            if data.len() < 19 {
                // 1 + 16 + 2
                return Err(ProtocolError::InvalidFrame("IPv6 dest too short".into()));
            }
            let octets: [u8; 16] = data[1..17].try_into().unwrap();
            let addr = Ipv6Addr::from(octets);
            let port = u16::from_be_bytes([data[17], data[18]]);
            Ok(ProxyDestination {
                address: ProxyAddress::Ipv6(addr),
                port,
            })
        }
        _ => Err(ProtocolError::InvalidAddressType(addr_type)),
    }
}

// --- Encrypted frame wire format ---
// [nonce:12][len:2][ciphertext][tag:16]

use crate::crypto::aead::AeadCipher;

/// Encrypt a plaintext data frame into the wire format.
pub fn encrypt_frame(
    cipher: &dyn AeadCipher,
    nonce: &[u8; NONCE_SIZE],
    plaintext: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    let ciphertext = cipher.encrypt(nonce, plaintext, &[])?;
    let len = ciphertext.len() as u16;
    let mut wire = Vec::with_capacity(NONCE_SIZE + 2 + ciphertext.len());
    wire.extend_from_slice(nonce);
    wire.extend_from_slice(&len.to_be_bytes());
    wire.extend_from_slice(&ciphertext);
    Ok(wire)
}

/// Decrypt a wire-format encrypted frame.
pub fn decrypt_frame(
    cipher: &dyn AeadCipher,
    wire: &[u8],
) -> Result<(Vec<u8>, [u8; NONCE_SIZE]), CryptoError> {
    if wire.len() < NONCE_SIZE + 2 {
        return Err(CryptoError::DecryptionFailed(
            "Encrypted frame too short".into(),
        ));
    }
    let mut nonce = [0u8; NONCE_SIZE];
    nonce.copy_from_slice(&wire[..NONCE_SIZE]);
    let len = u16::from_be_bytes([wire[NONCE_SIZE], wire[NONCE_SIZE + 1]]) as usize;
    let ciphertext_start = NONCE_SIZE + 2;
    if wire.len() < ciphertext_start + len {
        return Err(CryptoError::DecryptionFailed(
            "Encrypted frame truncated".into(),
        ));
    }
    let ciphertext = &wire[ciphertext_start..ciphertext_start + len];
    let plaintext = cipher.decrypt(&nonce, ciphertext, &[])?;
    Ok((plaintext, nonce))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::PRISMA_PROTOCOL_VERSION;

    #[test]
    fn test_client_init_round_trip() {
        let msg = PrismaClientInit {
            version: PRISMA_PROTOCOL_VERSION,
            flags: 0,
            client_ephemeral_pub: [0xAA; 32],
            client_id: ClientId(uuid::Uuid::nil()),
            timestamp: 1700000000,
            cipher_suite: CipherSuite::ChaCha20Poly1305,
            auth_token: [0xBB; 32],
            padding: vec![1, 2, 3],
        };
        let encoded = encode_client_init(&msg);
        let decoded = decode_client_init(&encoded).unwrap();
        assert_eq!(decoded.version, msg.version);
        assert_eq!(decoded.flags, msg.flags);
        assert_eq!(decoded.client_ephemeral_pub, msg.client_ephemeral_pub);
        assert_eq!(decoded.client_id, msg.client_id);
        assert_eq!(decoded.timestamp, msg.timestamp);
        assert_eq!(decoded.cipher_suite, msg.cipher_suite);
        assert_eq!(decoded.auth_token, msg.auth_token);
        assert_eq!(decoded.padding, msg.padding);
    }

    #[test]
    fn test_client_init_minimum_size() {
        // Exactly 91 bytes: should succeed with empty padding
        let msg = PrismaClientInit {
            version: PRISMA_PROTOCOL_VERSION,
            flags: 0,
            client_ephemeral_pub: [0xAA; 32],
            client_id: ClientId(uuid::Uuid::nil()),
            timestamp: 1700000000,
            cipher_suite: CipherSuite::ChaCha20Poly1305,
            auth_token: [0xBB; 32],
            padding: vec![],
        };
        let encoded = encode_client_init(&msg);
        assert_eq!(encoded.len(), 91);
        let decoded = decode_client_init(&encoded).unwrap();
        assert_eq!(decoded.padding, Vec::<u8>::new());
    }

    #[test]
    fn test_client_init_too_short() {
        let data = [0u8; 90];
        assert!(decode_client_init(&data).is_err());
    }

    #[test]
    fn test_server_init_round_trip() {
        let msg = PrismaServerInit {
            status: AcceptStatus::Ok,
            session_id: uuid::Uuid::nil(),
            server_ephemeral_pub: [0xCC; 32],
            challenge: [0xDD; 32],
            padding_min: 10,
            padding_max: 200,
            server_features: FEATURE_UDP_RELAY | FEATURE_SPEED_TEST,
            session_ticket: vec![1, 2, 3, 4, 5],
            bucket_sizes: vec![128, 256, 512],
            padding: vec![6, 7, 8],
        };
        let encoded = encode_server_init(&msg);
        let decoded = decode_server_init(&encoded).unwrap();
        assert_eq!(decoded.status, msg.status);
        assert_eq!(decoded.session_id, msg.session_id);
        assert_eq!(decoded.server_ephemeral_pub, msg.server_ephemeral_pub);
        assert_eq!(decoded.challenge, msg.challenge);
        assert_eq!(decoded.padding_min, msg.padding_min);
        assert_eq!(decoded.padding_max, msg.padding_max);
        assert_eq!(decoded.server_features, msg.server_features);
        assert_eq!(decoded.session_ticket, msg.session_ticket);
        assert_eq!(decoded.bucket_sizes, msg.bucket_sizes);
        assert_eq!(decoded.padding, msg.padding);
    }

    #[test]
    fn test_server_init_no_buckets_round_trip() {
        let msg = PrismaServerInit {
            status: AcceptStatus::Ok,
            session_id: uuid::Uuid::nil(),
            server_ephemeral_pub: [0xCC; 32],
            challenge: [0xDD; 32],
            padding_min: 10,
            padding_max: 200,
            server_features: 0,
            session_ticket: vec![],
            bucket_sizes: vec![],
            padding: vec![],
        };
        let encoded = encode_server_init(&msg);
        let decoded = decode_server_init(&encoded).unwrap();
        assert_eq!(decoded.bucket_sizes, Vec::<u16>::new());
        assert_eq!(decoded.session_ticket, Vec::<u8>::new());
        assert_eq!(decoded.padding, Vec::<u8>::new());
    }

    #[test]
    fn test_server_init_too_short() {
        let data = [0u8; 92];
        assert!(decode_server_init(&data).is_err());
    }

    #[test]
    fn test_client_resume_round_trip() {
        let msg = PrismaClientResume {
            version: PRISMA_PROTOCOL_VERSION,
            flags: CLIENT_INIT_FLAG_RESUMPTION,
            client_ephemeral_pub: [0xEE; 32],
            session_ticket: vec![1, 2, 3, 4, 5],
            encrypted_0rtt_data: vec![10, 20, 30],
        };
        let encoded = encode_client_resume(&msg);
        let decoded = decode_client_resume(&encoded).unwrap();
        assert_eq!(decoded.version, msg.version);
        assert_eq!(decoded.flags, msg.flags);
        assert_eq!(decoded.client_ephemeral_pub, msg.client_ephemeral_pub);
        assert_eq!(decoded.session_ticket, msg.session_ticket);
        assert_eq!(decoded.encrypted_0rtt_data, msg.encrypted_0rtt_data);
    }

    #[test]
    fn test_session_ticket_round_trip() {
        use crate::types::PaddingRange;
        let ticket = SessionTicket {
            client_id: ClientId(uuid::Uuid::nil()),
            session_key: [0xAA; 32],
            cipher_suite: CipherSuite::Aes256Gcm,
            issued_at: 1700000000,
            padding_range: PaddingRange::new(10, 256),
        };
        let encoded = encode_session_ticket(&ticket);
        let decoded = decode_session_ticket(&encoded).unwrap();
        assert_eq!(decoded.client_id, ticket.client_id);
        assert_eq!(decoded.session_key, ticket.session_key);
        assert_eq!(decoded.cipher_suite, ticket.cipher_suite);
        assert_eq!(decoded.issued_at, ticket.issued_at);
        assert_eq!(decoded.padding_range, ticket.padding_range);
    }

    #[test]
    fn test_padded_data_frame_round_trip() {
        use crate::types::PaddingRange;
        let frame = DataFrame {
            command: Command::Data(vec![1, 2, 3, 4, 5]),
            flags: FLAG_PADDED,
            stream_id: 42,
        };
        let range = PaddingRange::new(10, 50);
        let encoded = encode_data_frame_padded(&frame, &range);
        let decoded = decode_data_frame(&encoded).unwrap();
        assert_eq!(decoded.command, frame.command);
        assert_eq!(decoded.flags, frame.flags);
        assert_eq!(decoded.stream_id, frame.stream_id);
    }

    #[test]
    fn test_data_frame_connect_round_trip() {
        let frame = DataFrame {
            command: Command::Connect(ProxyDestination {
                address: ProxyAddress::Domain("example.com".into()),
                port: 443,
            }),
            flags: 0,
            stream_id: 1,
        };
        let encoded = encode_data_frame(&frame);
        let decoded = decode_data_frame(&encoded).unwrap();
        assert_eq!(decoded.command, frame.command);
        assert_eq!(decoded.stream_id, frame.stream_id);
    }

    #[test]
    fn test_data_frame_ipv4_round_trip() {
        let frame = DataFrame {
            command: Command::Connect(ProxyDestination {
                address: ProxyAddress::Ipv4(Ipv4Addr::new(1, 2, 3, 4)),
                port: 80,
            }),
            flags: 0,
            stream_id: 42,
        };
        let encoded = encode_data_frame(&frame);
        let decoded = decode_data_frame(&encoded).unwrap();
        assert_eq!(decoded.command, frame.command);
    }

    #[test]
    fn test_data_frame_ipv6_round_trip() {
        let frame = DataFrame {
            command: Command::Connect(ProxyDestination {
                address: ProxyAddress::Ipv6(Ipv6Addr::LOCALHOST),
                port: 8080,
            }),
            flags: 0,
            stream_id: 7,
        };
        let encoded = encode_data_frame(&frame);
        let decoded = decode_data_frame(&encoded).unwrap();
        assert_eq!(decoded.command, frame.command);
    }

    #[test]
    fn test_data_frame_data_round_trip() {
        let frame = DataFrame {
            command: Command::Data(vec![1, 2, 3, 4, 5]),
            flags: FLAG_PADDED,
            stream_id: 100,
        };
        let encoded = encode_data_frame(&frame);
        let decoded = decode_data_frame(&encoded).unwrap();
        assert_eq!(decoded.command, frame.command);
        assert_eq!(decoded.flags, frame.flags);
    }

    #[test]
    fn test_data_frame_ping_pong_round_trip() {
        for cmd in [Command::Ping(42), Command::Pong(42), Command::Close] {
            let frame = DataFrame {
                command: cmd.clone(),
                flags: 0,
                stream_id: 0,
            };
            let encoded = encode_data_frame(&frame);
            let decoded = decode_data_frame(&encoded).unwrap();
            assert_eq!(decoded.command, frame.command);
        }
    }

    #[test]
    fn test_command_challenge_response_round_trip() {
        let frame = DataFrame {
            command: Command::ChallengeResponse { hash: [0xAA; 32] },
            flags: 0,
            stream_id: 0,
        };
        let encoded = encode_data_frame(&frame);
        let decoded = decode_data_frame(&encoded).unwrap();
        assert_eq!(decoded.command, frame.command);
    }

    #[test]
    fn test_command_dns_round_trip() {
        let frame = DataFrame {
            command: Command::DnsQuery {
                query_id: 42,
                data: vec![1, 2, 3],
            },
            flags: 0,
            stream_id: 0,
        };
        let encoded = encode_data_frame(&frame);
        let decoded = decode_data_frame(&encoded).unwrap();
        assert_eq!(decoded.command, frame.command);

        let frame = DataFrame {
            command: Command::DnsResponse {
                query_id: 42,
                data: vec![4, 5, 6],
            },
            flags: 0,
            stream_id: 0,
        };
        let encoded = encode_data_frame(&frame);
        let decoded = decode_data_frame(&encoded).unwrap();
        assert_eq!(decoded.command, frame.command);
    }

    #[test]
    fn test_command_speed_test_round_trip() {
        let frame = DataFrame {
            command: Command::SpeedTest {
                direction: 0,
                duration_secs: 10,
                data: vec![0xFF; 100],
            },
            flags: 0,
            stream_id: 0,
        };
        let encoded = encode_data_frame(&frame);
        let decoded = decode_data_frame(&encoded).unwrap();
        assert_eq!(decoded.command, frame.command);
    }

    #[test]
    fn test_encrypted_frame_round_trip() {
        use crate::crypto::aead::create_cipher;

        let key = [0x42u8; 32];
        let cipher = create_cipher(CipherSuite::ChaCha20Poly1305, &key);
        let nonce = [0u8; NONCE_SIZE];
        let plaintext = b"hello encrypted world";

        let wire = encrypt_frame(cipher.as_ref(), &nonce, plaintext).unwrap();
        let (decrypted, dec_nonce) = decrypt_frame(cipher.as_ref(), &wire).unwrap();
        assert_eq!(decrypted, plaintext);
        assert_eq!(dec_nonce, nonce);
    }
}
