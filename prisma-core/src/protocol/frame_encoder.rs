//! Zero-copy frame encoder/decoder for the hot relay path.
//!
//! Pre-allocates scratch buffers to eliminate per-frame heap allocations.
//! Uses in-place encryption/decryption to avoid copying data between buffers.

use crate::crypto::aead::{AeadCipher, TAG_SIZE};
use crate::error::CryptoError;
use crate::types::{PaddingRange, MAX_FRAME_SIZE, NONCE_SIZE};

use super::types::{CMD_DATA, FLAG_PADDED};

/// Frame header size: cmd(1) + flags(2) + stream_id(4) + payload_len(2) = 9
const PADDED_HEADER_SIZE: usize = 9;

/// Wire overhead: outer_len(2) + nonce(12) + inner_len(2) + tag(16) = 32
const WIRE_OVERHEAD: usize = 2 + NONCE_SIZE + 2 + TAG_SIZE;

/// Maximum payload bytes that can safely be written into `payload_mut()`.
///
/// Accounts for frame header and max possible padding so that the
/// resulting wire frame never exceeds `MAX_FRAME_SIZE`.
pub const MAX_PAYLOAD_SIZE: usize = MAX_FRAME_SIZE
    - NONCE_SIZE
    - 2
    - TAG_SIZE
    - PADDED_HEADER_SIZE
    - crate::types::MAX_PADDING_SIZE;

/// Pre-allocated frame encoder for the send (encrypt) direction.
///
/// Eliminates all hot-path heap allocations by encoding the frame header,
/// payload, and padding directly into a single buffer, then encrypting in-place.
pub struct FrameEncoder {
    buf: Vec<u8>,
}

impl FrameEncoder {
    pub fn new() -> Self {
        // Max wire frame: 2 (outer_len) + 12 (nonce) + 2 (inner_len) + MAX_FRAME_SIZE + 16 (tag)
        Self {
            buf: vec![0u8; WIRE_OVERHEAD + MAX_FRAME_SIZE],
        }
    }

    /// Returns a mutable slice where the caller should write payload data.
    /// The slice starts at the correct offset within the internal buffer.
    ///
    /// After writing `n` bytes into this slice, call `seal_data_frame` with `n`.
    pub fn payload_mut(&mut self) -> &mut [u8] {
        let payload_start = 2 + NONCE_SIZE + 2 + PADDED_HEADER_SIZE;
        let payload_end = self.buf.len() - TAG_SIZE;
        &mut self.buf[payload_start..payload_end]
    }

    /// Encode and encrypt a data frame in-place. The payload must have been
    /// written into `payload_mut()[..payload_len]` before calling this.
    ///
    /// Returns a slice of the internal buffer containing the complete wire frame,
    /// including the 2-byte outer length prefix (ready for a single `write_all`).
    pub fn seal_data_frame(
        &mut self,
        cipher: &dyn AeadCipher,
        nonce: &[u8; NONCE_SIZE],
        payload_len: usize,
        stream_id: u32,
        padding_range: &PaddingRange,
    ) -> Result<&[u8], CryptoError> {
        // Compute padding length
        let pad_len = if padding_range.max > 0 {
            padding_range.random_in_range()
        } else {
            0
        };

        let plaintext_len = PADDED_HEADER_SIZE + payload_len + pad_len;

        // Layout: [outer_len:2][nonce:12][inner_len:2][plaintext...][tag:16]
        let nonce_start = 2;
        let inner_len_start = nonce_start + NONCE_SIZE; // 14
        let plaintext_start = inner_len_start + 2; // 16

        // Write frame header at the correct position
        self.buf[plaintext_start] = CMD_DATA;
        self.buf[plaintext_start + 1..plaintext_start + 3]
            .copy_from_slice(&FLAG_PADDED.to_le_bytes());
        self.buf[plaintext_start + 3..plaintext_start + 7]
            .copy_from_slice(&stream_id.to_be_bytes());
        self.buf[plaintext_start + 7..plaintext_start + 9]
            .copy_from_slice(&(payload_len as u16).to_be_bytes());

        // Payload is already at position plaintext_start + 9 (written by caller via payload_mut)

        // Zero-fill padding (Phase 4: no RNG needed, encrypted anyway)
        let pad_start = plaintext_start + PADDED_HEADER_SIZE + payload_len;
        self.buf[pad_start..pad_start + pad_len].fill(0);

        let plaintext_end = plaintext_start + plaintext_len;

        // Encrypt plaintext in-place
        let tag =
            cipher.encrypt_in_place(nonce, &[], &mut self.buf[plaintext_start..plaintext_end])?;

        // Write tag after ciphertext
        self.buf[plaintext_end..plaintext_end + TAG_SIZE].copy_from_slice(&tag);

        // Write nonce
        self.buf[nonce_start..nonce_start + NONCE_SIZE].copy_from_slice(nonce);

        // Write inner_len = ciphertext + tag = plaintext_len + TAG_SIZE
        let inner_len = (plaintext_len + TAG_SIZE) as u16;
        self.buf[inner_len_start..inner_len_start + 2].copy_from_slice(&inner_len.to_be_bytes());

        // Write outer_len = nonce + inner_len_field + inner_len
        let outer_len = (NONCE_SIZE + 2 + plaintext_len + TAG_SIZE) as u16;
        self.buf[0..2].copy_from_slice(&outer_len.to_be_bytes());

        let total = 2 + outer_len as usize;
        Ok(&self.buf[..total])
    }
}

impl Default for FrameEncoder {
    fn default() -> Self {
        Self::new()
    }
}

/// Pre-allocated frame decoder for the receive (decrypt) direction.
///
/// Decrypts encrypted frames in-place within the provided buffer,
/// avoiding allocation of a separate plaintext buffer.
pub struct FrameDecoder;

impl FrameDecoder {
    /// Decrypt a frame in-place and extract the data payload.
    ///
    /// `frame_buf` must contain `[nonce:12][inner_len:2][ciphertext:var][tag:16]`.
    /// After this call, the ciphertext region is replaced with plaintext.
    ///
    /// Returns `(command_byte, payload_slice, nonce)`:
    /// - For CMD_DATA: payload_slice is the raw data (frame header stripped)
    /// - For other commands: payload_slice is the full decrypted plaintext
    pub fn unseal_data_frame<'a>(
        frame_buf: &'a mut [u8],
        frame_len: usize,
        cipher: &dyn AeadCipher,
    ) -> Result<(u8, &'a [u8], [u8; NONCE_SIZE]), CryptoError> {
        if frame_len < NONCE_SIZE + 2 {
            return Err(CryptoError::DecryptionFailed(
                "Encrypted frame too short".into(),
            ));
        }
        let mut nonce = [0u8; NONCE_SIZE];
        nonce.copy_from_slice(&frame_buf[..NONCE_SIZE]);
        let inner_len =
            u16::from_be_bytes([frame_buf[NONCE_SIZE], frame_buf[NONCE_SIZE + 1]]) as usize;
        let ciphertext_start = NONCE_SIZE + 2;

        if frame_len < ciphertext_start + inner_len || inner_len < TAG_SIZE {
            return Err(CryptoError::DecryptionFailed(
                "Encrypted frame truncated".into(),
            ));
        }

        let data_len = inner_len - TAG_SIZE;
        let tag_start = ciphertext_start + data_len;

        let mut tag = [0u8; TAG_SIZE];
        tag.copy_from_slice(&frame_buf[tag_start..tag_start + TAG_SIZE]);

        cipher.decrypt_in_place(
            &nonce,
            &[],
            &mut frame_buf[ciphertext_start..ciphertext_start + data_len],
            &tag,
        )?;

        // Now frame_buf[ciphertext_start..ciphertext_start + data_len] is plaintext
        let plaintext = &frame_buf[ciphertext_start..ciphertext_start + data_len];

        if plaintext.is_empty() {
            return Err(CryptoError::DecryptionFailed("Empty plaintext".into()));
        }

        // Extract command byte
        let cmd = plaintext[0];

        // For CMD_DATA: extract payload directly (skip frame header)
        if cmd == CMD_DATA && plaintext.len() >= 7 {
            let flags = u16::from_le_bytes([plaintext[1], plaintext[2]]);
            if flags & FLAG_PADDED != 0 && plaintext.len() >= 9 {
                let payload_len = u16::from_be_bytes([plaintext[7], plaintext[8]]) as usize;
                if plaintext.len() >= 9 + payload_len {
                    return Ok((cmd, &plaintext[9..9 + payload_len], nonce));
                }
            } else {
                return Ok((cmd, &plaintext[7..], nonce));
            }
        }

        // For non-DATA frames, return full plaintext for decode_data_frame
        Ok((cmd, plaintext, nonce))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::aead::create_cipher;
    use crate::types::CipherSuite;

    #[test]
    fn test_frame_encoder_round_trip() {
        let key = [0x42u8; 32];
        let cipher = create_cipher(CipherSuite::ChaCha20Poly1305, &key);
        let padding_range = PaddingRange::new(0, 0); // No padding for simpler test

        let mut encoder = FrameEncoder::new();
        let payload = b"hello encoder";
        let nonce = [0u8; NONCE_SIZE];

        // Write payload into encoder buffer
        encoder.payload_mut()[..payload.len()].copy_from_slice(payload);

        // Seal the frame
        let wire = encoder
            .seal_data_frame(cipher.as_ref(), &nonce, payload.len(), 42, &padding_range)
            .unwrap();

        // Parse the wire frame
        let outer_len = u16::from_be_bytes([wire[0], wire[1]]) as usize;
        assert_eq!(wire.len(), 2 + outer_len);

        // Decrypt using the standard decrypt_frame
        let mut frame_buf = wire[2..].to_vec();
        let (cmd, plaintext_slice, dec_nonce) =
            FrameDecoder::unseal_data_frame(&mut frame_buf, outer_len, cipher.as_ref()).unwrap();
        assert_eq!(dec_nonce, nonce);
        assert_eq!(cmd, CMD_DATA);
        assert_eq!(plaintext_slice, payload);
    }

    #[test]
    fn test_frame_encoder_with_padding() {
        let key = [0x42u8; 32];
        let cipher = create_cipher(CipherSuite::ChaCha20Poly1305, &key);
        let padding_range = PaddingRange::new(10, 50);

        let mut encoder = FrameEncoder::new();
        let payload = b"padded payload";
        let nonce = [1u8; NONCE_SIZE];

        encoder.payload_mut()[..payload.len()].copy_from_slice(payload);
        let wire = encoder
            .seal_data_frame(cipher.as_ref(), &nonce, payload.len(), 0, &padding_range)
            .unwrap();

        let outer_len = u16::from_be_bytes([wire[0], wire[1]]) as usize;
        let mut frame_buf = wire[2..].to_vec();
        let (_cmd, plaintext_slice, _nonce) =
            FrameDecoder::unseal_data_frame(&mut frame_buf, outer_len, cipher.as_ref()).unwrap();
        assert_eq!(plaintext_slice, payload);
    }

    #[test]
    fn test_frame_encoder_transport_only() {
        let key = [0x42u8; 32];
        let cipher = create_cipher(CipherSuite::TransportOnly, &key);
        let padding_range = PaddingRange::new(0, 0);

        let mut encoder = FrameEncoder::new();
        let payload = b"transport only test";
        let nonce = [2u8; NONCE_SIZE];

        encoder.payload_mut()[..payload.len()].copy_from_slice(payload);
        let wire = encoder
            .seal_data_frame(cipher.as_ref(), &nonce, payload.len(), 0, &padding_range)
            .unwrap();

        let outer_len = u16::from_be_bytes([wire[0], wire[1]]) as usize;
        let mut frame_buf = wire[2..].to_vec();
        let (_cmd, plaintext_slice, _nonce) =
            FrameDecoder::unseal_data_frame(&mut frame_buf, outer_len, cipher.as_ref()).unwrap();
        assert_eq!(plaintext_slice, payload);
    }
}
