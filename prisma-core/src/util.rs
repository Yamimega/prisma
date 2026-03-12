use hmac::{Hmac, Mac};
use sha2::Sha256;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::error::{ConfigError, ProtocolError};
use crate::types::{ClientId, MAX_FRAME_SIZE};

type HmacSha256 = Hmac<Sha256>;

// --- Hex encoding/decoding ---

/// Decode a hex string into bytes. Returns `None` if the string has odd length
/// or contains non-hex characters.
pub fn hex_decode(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}

/// Decode a hex string into a 32-byte array. Returns an error if the string
/// is not valid hex or not exactly 64 hex characters (32 bytes).
pub fn hex_decode_32(s: &str) -> Result<[u8; 32], ConfigError> {
    let bytes = hex_decode(s).ok_or_else(|| ConfigError::ValidationFailed("invalid hex".into()))?;
    if bytes.len() != 32 {
        return Err(ConfigError::ValidationFailed(format!(
            "expected 32 bytes (64 hex chars), got {} bytes",
            bytes.len()
        )));
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Ok(arr)
}

// --- Auth token computation ---

/// Compute the HMAC-SHA256 auth token: `HMAC(auth_secret, client_id || timestamp)`.
/// Used by both client (to produce) and server (to verify).
pub fn compute_auth_token(
    auth_secret: &[u8; 32],
    client_id: &ClientId,
    timestamp: u64,
) -> [u8; 32] {
    let mut mac = HmacSha256::new_from_slice(auth_secret).expect("HMAC key length is valid");
    mac.update(client_id.0.as_bytes());
    mac.update(&timestamp.to_be_bytes());
    mac.finalize().into_bytes().into()
}

// --- Length-prefixed framed I/O ---

/// Read a length-prefixed frame: `[len:2][payload:len]`.
/// Rejects frames larger than `MAX_FRAME_SIZE`.
pub async fn read_framed<R: AsyncReadExt + Unpin>(r: &mut R) -> Result<Vec<u8>, ProtocolError> {
    let mut len_buf = [0u8; 2];
    r.read_exact(&mut len_buf)
        .await
        .map_err(|e| ProtocolError::InvalidFrame(format!("read length: {}", e)))?;
    let len = u16::from_be_bytes(len_buf) as usize;
    if len > MAX_FRAME_SIZE {
        return Err(ProtocolError::FrameTooLarge {
            size: len,
            max: MAX_FRAME_SIZE,
        });
    }
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf)
        .await
        .map_err(|e| ProtocolError::InvalidFrame(format!("read payload: {}", e)))?;
    Ok(buf)
}

/// Write a length-prefixed frame: `[len:2][payload]`.
pub async fn write_framed<W: AsyncWriteExt + Unpin>(
    w: &mut W,
    payload: &[u8],
) -> Result<(), ProtocolError> {
    let len = (payload.len() as u16).to_be_bytes();
    w.write_all(&len)
        .await
        .map_err(|e| ProtocolError::InvalidFrame(format!("write length: {}", e)))?;
    w.write_all(payload)
        .await
        .map_err(|e| ProtocolError::InvalidFrame(format!("write payload: {}", e)))?;
    Ok(())
}

// --- Constant-time comparison ---

/// Constant-time comparison of two 32-byte arrays to prevent timing side-channels.
pub fn ct_eq(a: &[u8; 32], b: &[u8; 32]) -> bool {
    use subtle::ConstantTimeEq;
    a.ct_eq(b).into()
}
