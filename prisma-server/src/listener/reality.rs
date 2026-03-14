//! REALITY TLS interception and mask server relay.
//!
//! Implements the REALITY protocol for active probing resistance:
//! - Server forwards ClientHello to a real "mask" server (e.g., www.microsoft.com)
//! - Authenticated clients are identified via encrypted data in TLS Session ID
//! - Unauthenticated connections (probers) are transparently proxied to the mask server
//! - The server is indistinguishable from the real website to any prober

use std::net::ToSocketAddrs;

use anyhow::Result;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tracing::debug;

use prisma_core::config::server::PrismaTlsConfig;

/// Minimum TLS ClientHello size (record header + handshake header + minimal content).
const MIN_TLS_CLIENT_HELLO: usize = 5;
/// TLS record type: Handshake
const TLS_HANDSHAKE: u8 = 0x16;
/// TLS handshake type: ClientHello
const HANDSHAKE_CLIENT_HELLO: u8 = 0x01;
/// Offset of Session ID length in a ClientHello (after TLS record header + handshake header).
/// TLS record: [type:1][version:2][length:2]
/// Handshake: [type:1][length:3][client_version:2][random:32]
/// Session ID length is at offset 5 + 4 + 2 + 32 = 43
const SESSION_ID_LENGTH_OFFSET: usize = 43;

/// REALITY session ID wire format:
/// `[version:1][short_id:8][timestamp:4][reserved:3][encrypted_auth:16]`
const REALITY_SESSION_ID_SIZE: usize = 32;
const REALITY_VERSION_BYTE: u8 = 0x04;

/// Parsed REALITY authentication from TLS Session ID.
#[derive(Debug)]
pub struct RealityAuth {
    pub short_id: [u8; 8],
    pub timestamp: u32,
    pub encrypted_auth: [u8; 16],
}

/// Extract and validate REALITY auth from a TLS ClientHello's Session ID.
///
/// Returns `Some(RealityAuth)` if the Session ID contains a REALITY version byte,
/// `None` if it's a normal ClientHello (should be proxied to mask server).
///
/// Note: Full auth verification is now handled by the PrismaAuth module.
/// This function only checks if the TLS ClientHello has a session ID with the
/// REALITY version byte and extracts the fields.
pub fn extract_reality_auth(client_hello: &[u8], _config: &PrismaTlsConfig) -> Option<RealityAuth> {
    // Verify this is a TLS ClientHello
    if client_hello.len() < SESSION_ID_LENGTH_OFFSET + 1 {
        return None;
    }
    if client_hello[0] != TLS_HANDSHAKE {
        return None;
    }
    // Check handshake type after TLS record header (5 bytes)
    if client_hello.len() < 6 || client_hello[5] != HANDSHAKE_CLIENT_HELLO {
        return None;
    }

    // Extract Session ID
    let session_id_len = client_hello[SESSION_ID_LENGTH_OFFSET] as usize;
    if session_id_len != REALITY_SESSION_ID_SIZE {
        return None;
    }

    let sid_start = SESSION_ID_LENGTH_OFFSET + 1;
    let sid_end = sid_start + REALITY_SESSION_ID_SIZE;
    if client_hello.len() < sid_end {
        return None;
    }

    let session_id = &client_hello[sid_start..sid_end];

    // Check version byte
    if session_id[0] != REALITY_VERSION_BYTE {
        return None;
    }

    // Extract short_id
    let mut short_id = [0u8; 8];
    short_id.copy_from_slice(&session_id[1..9]);

    // Extract timestamp
    let timestamp = u32::from_be_bytes([
        session_id[9],
        session_id[10],
        session_id[11],
        session_id[12],
    ]);

    // Extract encrypted auth
    let mut encrypted_auth = [0u8; 16];
    encrypted_auth.copy_from_slice(&session_id[16..32]);

    Some(RealityAuth {
        short_id,
        timestamp,
        encrypted_auth,
    })
}

/// Extract the SNI (Server Name Indication) from a TLS ClientHello.
pub fn extract_sni(client_hello: &[u8]) -> Option<String> {
    // This is a simplified SNI extractor. In production, you'd want
    // a proper TLS parser, but this handles the common case.
    if client_hello.len() < MIN_TLS_CLIENT_HELLO {
        return None;
    }
    if client_hello[0] != TLS_HANDSHAKE {
        return None;
    }

    // Find the SNI extension (type 0x0000)
    // We search for the pattern [0x00, 0x00, len_hi, len_lo, 0x00, name_len_hi, name_len_lo, 0x00, ...]
    // This is a heuristic approach — a full parser would be more robust
    let data = client_hello;
    for i in 0..data.len().saturating_sub(9) {
        // Look for extension type 0x0000 (server_name)
        if data[i] == 0x00 && data[i + 1] == 0x00 {
            let ext_len = u16::from_be_bytes([data[i + 2], data[i + 3]]) as usize;
            if ext_len == 0 || i + 4 + ext_len > data.len() {
                continue;
            }
            // server_name_list_length
            if i + 6 >= data.len() {
                continue;
            }
            let list_len = u16::from_be_bytes([data[i + 4], data[i + 5]]) as usize;
            if list_len == 0 || i + 6 + list_len > data.len() {
                continue;
            }
            // name_type should be 0x00 (host_name)
            if data[i + 6] != 0x00 {
                continue;
            }
            if i + 9 > data.len() {
                continue;
            }
            let name_len = u16::from_be_bytes([data[i + 7], data[i + 8]]) as usize;
            if name_len == 0 || i + 9 + name_len > data.len() {
                continue;
            }
            if let Ok(name) = std::str::from_utf8(&data[i + 9..i + 9 + name_len]) {
                // Validate that it looks like a hostname
                if name.contains('.') && name.len() > 3 {
                    return Some(name.to_string());
                }
            }
        }
    }
    None
}

/// Relay a connection transparently to the REALITY mask server.
///
/// This makes probers see exactly what the real mask server would respond with.
pub async fn relay_to_mask(
    mut client: TcpStream,
    client_hello_bytes: &[u8],
    config: &PrismaTlsConfig,
) -> Result<()> {
    let first_mask = config
        .mask_servers
        .first()
        .ok_or_else(|| anyhow::anyhow!("No mask servers configured in PrismaTLS"))?;
    let dest = &first_mask.addr;
    debug!(dest = %dest, "REALITY: relaying to mask server");

    // Resolve mask server address
    let addr = dest
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| anyhow::anyhow!("Failed to resolve mask server: {}", dest))?;

    let mut mask_server = TcpStream::connect(addr).await?;

    // Forward the ClientHello that was already read
    mask_server.write_all(client_hello_bytes).await?;

    // Bidirectional copy
    tokio::io::copy_bidirectional(&mut client, &mut mask_server).await?;

    Ok(())
}

/// Check if a server name matches the PrismaTLS config's allowed server names.
pub fn is_allowed_server_name(sni: &str, config: &PrismaTlsConfig) -> bool {
    config.mask_servers.iter().any(|server| {
        server.names.iter().any(|name| {
            if name.starts_with('.') {
                // Wildcard match: ".example.com" matches "sub.example.com"
                sni.ends_with(name)
            } else {
                sni == name
            }
        })
    })
}

/// Build the REALITY Session ID for a v4 client.
///
/// Packs auth data into a 32-byte TLS Session ID:
/// `[version:1][short_id:8][timestamp:4][reserved:3][encrypted_auth:16]`
///
/// **Deprecated**: This function is kept for backward compatibility.
/// New code should use the PrismaAuth module instead.
pub fn build_reality_session_id(
    short_id: &[u8; 8],
    timestamp: u32,
    encrypted_auth: &[u8; 16],
) -> [u8; 32] {
    let mut session_id = [0u8; 32];
    session_id[0] = REALITY_VERSION_BYTE;
    session_id[1..9].copy_from_slice(short_id);
    session_id[9..13].copy_from_slice(&timestamp.to_be_bytes());
    // reserved[13..16] stays zeroed
    session_id[16..32].copy_from_slice(encrypted_auth);
    session_id
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_reality_session_id() {
        let short_id = [0xAB; 8];
        let timestamp = 1700000000u32;
        let encrypted_auth = [0xCD; 16];

        let sid = build_reality_session_id(&short_id, timestamp, &encrypted_auth);

        assert_eq!(sid[0], REALITY_VERSION_BYTE);
        assert_eq!(&sid[1..9], &short_id);
        assert_eq!(
            u32::from_be_bytes([sid[9], sid[10], sid[11], sid[12]]),
            timestamp
        );
        assert_eq!(&sid[16..32], &encrypted_auth);
    }

    #[test]
    fn test_is_allowed_server_name() {
        use prisma_core::config::server::MaskServerEntry;

        let config = PrismaTlsConfig {
            enabled: true,
            mask_servers: vec![MaskServerEntry {
                addr: "www.microsoft.com:443".into(),
                names: vec!["www.microsoft.com".into(), ".azure.com".into()],
            }],
            ..Default::default()
        };

        assert!(is_allowed_server_name("www.microsoft.com", &config));
        assert!(is_allowed_server_name("portal.azure.com", &config));
        assert!(!is_allowed_server_name("www.google.com", &config));
    }

    #[test]
    fn test_extract_reality_auth_wrong_version() {
        // Build a fake ClientHello with wrong REALITY version
        let mut hello = vec![0u8; 80];
        hello[0] = TLS_HANDSHAKE; // TLS record type
        hello[5] = HANDSHAKE_CLIENT_HELLO; // Handshake type
        hello[SESSION_ID_LENGTH_OFFSET] = 32; // Session ID length
        hello[SESSION_ID_LENGTH_OFFSET + 1] = 0x03; // Wrong version (v3 not v4)

        let config = PrismaTlsConfig {
            enabled: true,
            ..Default::default()
        };

        assert!(extract_reality_auth(&hello, &config).is_none());
    }
}
