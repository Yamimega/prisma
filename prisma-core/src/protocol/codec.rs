use std::net::{Ipv4Addr, Ipv6Addr};

use crate::error::{CryptoError, ProtocolError};
use crate::types::{CipherSuite, ClientId, ProxyAddress, ProxyDestination, NONCE_SIZE};

use super::types::*;

// --- Handshake message encoding/decoding ---

/// Encode ClientHello to bytes.
pub fn encode_client_hello(msg: &ClientHello) -> Vec<u8> {
    let mut buf = Vec::with_capacity(1 + 32 + 8 + msg.padding.len());
    buf.push(msg.version);
    buf.extend_from_slice(&msg.client_ephemeral_pub);
    buf.extend_from_slice(&msg.timestamp.to_be_bytes());
    buf.extend_from_slice(&msg.padding);
    buf
}

/// Decode ClientHello from bytes.
pub fn decode_client_hello(data: &[u8]) -> Result<ClientHello, ProtocolError> {
    if data.len() < 41 {
        // 1 + 32 + 8
        return Err(ProtocolError::InvalidFrame(
            "ClientHello too short".to_string(),
        ));
    }
    let version = data[0];
    let mut pub_key = [0u8; 32];
    pub_key.copy_from_slice(&data[1..33]);
    let timestamp = u64::from_be_bytes(data[33..41].try_into().unwrap());
    let padding = data[41..].to_vec();

    Ok(ClientHello {
        version,
        client_ephemeral_pub: pub_key,
        timestamp,
        padding,
    })
}

/// Encode ServerHello to bytes.
pub fn encode_server_hello(msg: &ServerHello) -> Vec<u8> {
    let mut buf = Vec::with_capacity(32 + 2 + msg.encrypted_challenge.len() + msg.padding.len());
    buf.extend_from_slice(&msg.server_ephemeral_pub);
    buf.extend_from_slice(&(msg.encrypted_challenge.len() as u16).to_be_bytes());
    buf.extend_from_slice(&msg.encrypted_challenge);
    buf.extend_from_slice(&msg.padding);
    buf
}

/// Decode ServerHello from bytes.
pub fn decode_server_hello(data: &[u8]) -> Result<ServerHello, ProtocolError> {
    if data.len() < 34 {
        // 32 + 2
        return Err(ProtocolError::InvalidFrame(
            "ServerHello too short".to_string(),
        ));
    }
    let mut pub_key = [0u8; 32];
    pub_key.copy_from_slice(&data[..32]);
    let challenge_len = u16::from_be_bytes([data[32], data[33]]) as usize;
    if data.len() < 34 + challenge_len {
        return Err(ProtocolError::InvalidFrame(
            "ServerHello challenge truncated".to_string(),
        ));
    }
    let encrypted_challenge = data[34..34 + challenge_len].to_vec();
    let padding = data[34 + challenge_len..].to_vec();

    Ok(ServerHello {
        server_ephemeral_pub: pub_key,
        encrypted_challenge,
        padding,
    })
}

/// Encode ClientAuth to bytes (plaintext, will be encrypted by handshake layer).
pub fn encode_client_auth(msg: &ClientAuth) -> Vec<u8> {
    let mut buf = Vec::with_capacity(16 + 32 + 1 + 32);
    buf.extend_from_slice(msg.client_id.0.as_bytes());
    buf.extend_from_slice(&msg.auth_token);
    buf.push(msg.cipher_suite as u8);
    buf.extend_from_slice(&msg.challenge_response);
    buf
}

/// Decode ClientAuth from bytes.
pub fn decode_client_auth(data: &[u8]) -> Result<ClientAuth, ProtocolError> {
    if data.len() < 81 {
        // 16 + 32 + 1 + 32
        return Err(ProtocolError::InvalidFrame(
            "ClientAuth too short".to_string(),
        ));
    }
    let client_id = ClientId(uuid::Uuid::from_bytes(data[..16].try_into().unwrap()));
    let mut auth_token = [0u8; 32];
    auth_token.copy_from_slice(&data[16..48]);
    let cipher_suite =
        CipherSuite::from_u8(data[48]).ok_or(ProtocolError::InvalidCommand(data[48]))?;
    let mut challenge_response = [0u8; 32];
    challenge_response.copy_from_slice(&data[49..81]);

    Ok(ClientAuth {
        client_id,
        auth_token,
        cipher_suite,
        challenge_response,
    })
}

/// Encode ServerAccept to bytes.
pub fn encode_server_accept(msg: &ServerAccept) -> Vec<u8> {
    let mut buf = Vec::with_capacity(1 + 16);
    buf.push(msg.status as u8);
    buf.extend_from_slice(msg.session_id.as_bytes());
    buf
}

/// Decode ServerAccept from bytes.
pub fn decode_server_accept(data: &[u8]) -> Result<ServerAccept, ProtocolError> {
    if data.len() < 17 {
        return Err(ProtocolError::InvalidFrame(
            "ServerAccept too short".to_string(),
        ));
    }
    let status = AcceptStatus::from_u8(data[0]).ok_or(ProtocolError::InvalidFrame(
        "Invalid accept status".to_string(),
    ))?;
    let session_id = uuid::Uuid::from_bytes(data[1..17].try_into().unwrap());
    Ok(ServerAccept { status, session_id })
}

// --- DataFrame encoding/decoding ---

/// Encode a DataFrame to plaintext bytes.
/// Format: [cmd:1][flags:1][stream_id:4][payload:var]
pub fn encode_data_frame(frame: &DataFrame) -> Vec<u8> {
    let payload = encode_command_payload(&frame.command);
    let mut buf = Vec::with_capacity(6 + payload.len());
    buf.push(frame.command.cmd_byte());
    buf.push(frame.flags);
    buf.extend_from_slice(&frame.stream_id.to_be_bytes());
    buf.extend_from_slice(&payload);
    buf
}

/// Decode a DataFrame from plaintext bytes.
pub fn decode_data_frame(data: &[u8]) -> Result<DataFrame, ProtocolError> {
    if data.len() < 6 {
        return Err(ProtocolError::InvalidFrame(
            "DataFrame too short".to_string(),
        ));
    }
    let cmd = data[0];
    let flags = data[1];
    let stream_id = u32::from_be_bytes(data[2..6].try_into().unwrap());
    let payload = &data[6..];

    let command = decode_command_payload(cmd, payload)?;

    Ok(DataFrame {
        command,
        flags,
        stream_id,
    })
}

fn encode_command_payload(cmd: &Command) -> Vec<u8> {
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
    use crate::types::PROTOCOL_VERSION;

    #[test]
    fn test_client_hello_round_trip() {
        let msg = ClientHello {
            version: PROTOCOL_VERSION,
            client_ephemeral_pub: [0xAA; 32],
            timestamp: 1234567890,
            padding: vec![0x00, 0x05, 1, 2, 3, 4, 5],
        };
        let encoded = encode_client_hello(&msg);
        let decoded = decode_client_hello(&encoded).unwrap();
        assert_eq!(decoded.version, msg.version);
        assert_eq!(decoded.client_ephemeral_pub, msg.client_ephemeral_pub);
        assert_eq!(decoded.timestamp, msg.timestamp);
        assert_eq!(decoded.padding, msg.padding);
    }

    #[test]
    fn test_server_hello_round_trip() {
        let msg = ServerHello {
            server_ephemeral_pub: [0xBB; 32],
            encrypted_challenge: vec![1, 2, 3, 4, 5],
            padding: vec![0, 0],
        };
        let encoded = encode_server_hello(&msg);
        let decoded = decode_server_hello(&encoded).unwrap();
        assert_eq!(decoded.server_ephemeral_pub, msg.server_ephemeral_pub);
        assert_eq!(decoded.encrypted_challenge, msg.encrypted_challenge);
        assert_eq!(decoded.padding, msg.padding);
    }

    #[test]
    fn test_client_auth_round_trip() {
        let msg = ClientAuth {
            client_id: ClientId(uuid::Uuid::nil()),
            auth_token: [0xCC; 32],
            cipher_suite: CipherSuite::ChaCha20Poly1305,
            challenge_response: [0xDD; 32],
        };
        let encoded = encode_client_auth(&msg);
        let decoded = decode_client_auth(&encoded).unwrap();
        assert_eq!(decoded.client_id, msg.client_id);
        assert_eq!(decoded.auth_token, msg.auth_token);
        assert_eq!(decoded.cipher_suite, msg.cipher_suite);
        assert_eq!(decoded.challenge_response, msg.challenge_response);
    }

    #[test]
    fn test_server_accept_round_trip() {
        let msg = ServerAccept {
            status: AcceptStatus::Ok,
            session_id: uuid::Uuid::nil(),
        };
        let encoded = encode_server_accept(&msg);
        let decoded = decode_server_accept(&encoded).unwrap();
        assert_eq!(decoded.status, msg.status);
        assert_eq!(decoded.session_id, msg.session_id);
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
