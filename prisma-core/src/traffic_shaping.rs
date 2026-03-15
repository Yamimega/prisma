//! Traffic shaping — anti-fingerprinting defenses for encrypted tunnels.
//!
//! Implements defenses against encapsulated TLS fingerprinting (USENIX 2024)
//! and related traffic analysis attacks:
//!
//! 1. **Bucket padding**: Pad frames to fixed sizes to eliminate size-based classification
//! 2. **Timing jitter**: Random delay on handshake-phase frames to break timing patterns
//! 3. **Frame coalescing**: Buffer small frames and merge them to hide packet boundaries
//! 4. **Chaff injection**: Dummy frames during idle periods for background noise

use rand::Rng;
use serde::{Deserialize, Serialize};

/// Traffic shaping configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrafficShapingConfig {
    /// Padding mode: "none", "random", "bucket"
    #[serde(default = "default_padding_mode")]
    pub padding_mode: String,
    /// Fixed bucket sizes for bucket padding mode.
    #[serde(default = "default_bucket_sizes")]
    pub bucket_sizes: Vec<u16>,
    /// Maximum timing jitter (ms) applied to handshake-phase frames.
    #[serde(default)]
    pub timing_jitter_ms: u32,
    /// Interval (ms) for chaff frame injection when idle.
    /// 0 = disabled.
    #[serde(default)]
    pub chaff_interval_ms: u32,
    /// Buffer window (ms) for frame coalescing.
    /// 0 = disabled.
    #[serde(default = "default_coalesce_window_ms")]
    pub coalesce_window_ms: u32,
}

impl Default for TrafficShapingConfig {
    fn default() -> Self {
        Self {
            padding_mode: default_padding_mode(),
            bucket_sizes: default_bucket_sizes(),
            timing_jitter_ms: 0,
            chaff_interval_ms: 0,
            coalesce_window_ms: default_coalesce_window_ms(),
        }
    }
}

fn default_padding_mode() -> String {
    "none".into()
}

fn default_bucket_sizes() -> Vec<u16> {
    vec![128, 256, 512, 1024, 2048, 4096, 8192, 16384]
}

fn default_coalesce_window_ms() -> u32 {
    0
}

/// Padding mode parsed from config string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaddingMode {
    /// No traffic shaping padding.
    None,
    /// Random padding (existing v3 behavior).
    Random,
    /// Bucket padding — pad to nearest fixed size from bucket_sizes list.
    Bucket,
}

impl PaddingMode {
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "bucket" => PaddingMode::Bucket,
            "random" => PaddingMode::Random,
            _ => PaddingMode::None,
        }
    }
}

/// Compute the bucket-padded size for a given payload size.
///
/// Returns the smallest bucket size >= payload_size.
/// If payload exceeds all buckets, returns MAX_FRAME_SIZE (16384).
pub fn bucket_pad_size(payload_size: usize, bucket_sizes: &[u16]) -> usize {
    for &bucket in bucket_sizes {
        if payload_size <= bucket as usize {
            return bucket as usize;
        }
    }
    // Fallback: use max frame size
    crate::types::MAX_FRAME_SIZE
}

/// Generate bucket padding bytes to fill a frame to the target bucket size.
///
/// Returns: (padding_bytes, bucket_pad_len)
/// The padding_bytes should be appended to the frame payload.
pub fn generate_bucket_padding(payload_size: usize, bucket_sizes: &[u16]) -> (Vec<u8>, u16) {
    let target = bucket_pad_size(payload_size, bucket_sizes);
    if target <= payload_size {
        return (Vec::new(), 0);
    }
    let pad_len = target - payload_size;
    // Zero-fill: encrypted anyway, random provides no extra security benefit.
    let padding = vec![0u8; pad_len];
    (padding, pad_len as u16)
}

/// Encode a v4 data frame with bucket padding.
///
/// v4 frame plaintext format when FLAG_BUCKETED is set:
/// `[cmd:1][flags:2][stream_id:4][bucket_pad_len:2][payload:var][bucket_padding:var]`
///
/// The bucket_pad_len field tells the decoder how many trailing bytes are padding.
pub fn encode_bucketed_frame(
    command_byte: u8,
    flags: u16,
    stream_id: u32,
    payload: &[u8],
    bucket_sizes: &[u16],
) -> Vec<u8> {
    use crate::protocol::types::FLAG_BUCKETED;

    // Calculate the total "inner" size without padding:
    // cmd(1) + flags(2) + stream_id(4) + bucket_pad_len(2) + payload
    let inner_size = 1 + 2 + 4 + 2 + payload.len();
    let target_size = bucket_pad_size(inner_size, bucket_sizes);
    let pad_len = target_size.saturating_sub(inner_size);

    let actual_flags = flags | FLAG_BUCKETED;

    let mut buf = Vec::with_capacity(target_size);
    buf.push(command_byte);
    buf.extend_from_slice(&actual_flags.to_le_bytes());
    buf.extend_from_slice(&stream_id.to_be_bytes());
    buf.extend_from_slice(&(pad_len as u16).to_be_bytes());
    buf.extend_from_slice(payload);

    // Zero-fill padding: encrypted anyway, random provides no extra security benefit.
    buf.resize(buf.len() + pad_len, 0);

    buf
}

/// Decode a v4 bucketed frame, stripping the bucket padding.
///
/// Returns the frame data without bucket padding (the bucket_pad_len field
/// is consumed to determine how many trailing bytes to discard).
pub fn decode_bucketed_frame(data: &[u8]) -> Result<(u8, u16, u32, Vec<u8>), &'static str> {
    // Minimum: cmd(1) + flags(2) + stream_id(4) + bucket_pad_len(2) = 9
    if data.len() < 9 {
        return Err("bucketed frame too short");
    }

    let cmd = data[0];
    let flags = u16::from_le_bytes([data[1], data[2]]);
    let stream_id = u32::from_be_bytes([data[3], data[4], data[5], data[6]]);
    let bucket_pad_len = u16::from_be_bytes([data[7], data[8]]) as usize;

    let payload_with_padding = &data[9..];
    if payload_with_padding.len() < bucket_pad_len {
        return Err("bucket_pad_len exceeds frame size");
    }

    let payload_end = payload_with_padding.len() - bucket_pad_len;
    let payload = payload_with_padding[..payload_end].to_vec();

    Ok((cmd, flags, stream_id, payload))
}

/// Generate a chaff (dummy) frame that the receiver should discard.
///
/// Chaff frames use FLAG_CHAFF and contain random payload to maintain
/// background traffic noise.
pub fn generate_chaff_frame(bucket_sizes: &[u16]) -> Vec<u8> {
    use crate::protocol::types::{CMD_DATA, FLAG_CHAFF};

    let mut rng = rand::thread_rng();
    // Random small payload (32-128 bytes)
    let payload_len: usize = rng.gen_range(32..=128);
    let payload: Vec<u8> = (0..payload_len).map(|_| rng.gen()).collect();

    encode_bucketed_frame(CMD_DATA, FLAG_CHAFF, 0, &payload, bucket_sizes)
}

/// Compute a random timing jitter value in milliseconds.
/// Returns 0 if max_jitter_ms is 0.
pub fn random_jitter_ms(max_jitter_ms: u32) -> u64 {
    if max_jitter_ms == 0 {
        return 0;
    }
    rand::thread_rng().gen_range(0..=max_jitter_ms) as u64
}

/// Frame coalescer — buffers small frames and merges them.
///
/// Accumulates frames during a coalescing window and outputs a single
/// merged frame. This hides the packet-size signature of inner TLS
/// handshakes.
pub struct FrameCoalescer {
    buffer: Vec<u8>,
    window_ms: u32,
    max_size: usize,
}

impl FrameCoalescer {
    pub fn new(window_ms: u32) -> Self {
        Self {
            buffer: Vec::new(),
            window_ms,
            max_size: crate::types::MAX_FRAME_SIZE,
        }
    }

    /// Add data to the coalescing buffer.
    /// Returns true if the buffer should be flushed (exceeds max size).
    pub fn push(&mut self, data: &[u8]) -> bool {
        self.buffer.extend_from_slice(data);
        self.buffer.len() >= self.max_size
    }

    /// Flush the buffer, returning accumulated data.
    pub fn flush(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.buffer)
    }

    /// Check if the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// Get the coalescing window duration.
    pub fn window_ms(&self) -> u32 {
        self.window_ms
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bucket_pad_size() {
        let buckets = vec![128, 256, 512, 1024];
        assert_eq!(bucket_pad_size(50, &buckets), 128);
        assert_eq!(bucket_pad_size(128, &buckets), 128);
        assert_eq!(bucket_pad_size(129, &buckets), 256);
        assert_eq!(bucket_pad_size(1000, &buckets), 1024);
        assert_eq!(
            bucket_pad_size(1025, &buckets),
            crate::types::MAX_FRAME_SIZE
        ); // fallback to MAX
    }

    #[test]
    fn test_generate_bucket_padding() {
        let buckets = vec![128, 256, 512];
        let (padding, pad_len) = generate_bucket_padding(100, &buckets);
        assert_eq!(pad_len, 28); // 128 - 100
        assert_eq!(padding.len(), 28);
    }

    #[test]
    fn test_bucketed_frame_roundtrip() {
        let buckets = vec![128, 256, 512, 1024];
        let payload = b"Hello, World!";
        let frame = encode_bucketed_frame(0x02, 0, 42, payload, &buckets);

        let (cmd, flags, stream_id, decoded_payload) = decode_bucketed_frame(&frame).unwrap();
        assert_eq!(cmd, 0x02);
        assert!(flags & crate::protocol::types::FLAG_BUCKETED != 0);
        assert_eq!(stream_id, 42);
        assert_eq!(decoded_payload, payload);
    }

    #[test]
    fn test_bucketed_frame_size_is_bucket() {
        let buckets = vec![128, 256, 512, 1024];
        let payload = b"test data";
        let frame = encode_bucketed_frame(0x02, 0, 0, payload, &buckets);

        // Frame size should be one of the bucket sizes
        assert!(
            buckets.iter().any(|&b| frame.len() == b as usize) || frame.len() == 16384,
            "Frame size {} not in bucket set",
            frame.len()
        );
    }

    #[test]
    fn test_chaff_frame() {
        let buckets = vec![128, 256, 512];
        let frame = generate_chaff_frame(&buckets);
        let (cmd, flags, _stream_id, _payload) = decode_bucketed_frame(&frame).unwrap();
        assert_eq!(cmd, crate::protocol::types::CMD_DATA);
        assert!(flags & crate::protocol::types::FLAG_CHAFF != 0);
    }

    #[test]
    fn test_random_jitter() {
        assert_eq!(random_jitter_ms(0), 0);
        let jitter = random_jitter_ms(50);
        assert!(jitter <= 50);
    }

    #[test]
    fn test_frame_coalescer() {
        let mut coalescer = FrameCoalescer::new(5);
        assert!(coalescer.is_empty());

        let should_flush = coalescer.push(b"hello");
        assert!(!should_flush);
        assert!(!coalescer.is_empty());

        let should_flush = coalescer.push(b" world");
        assert!(!should_flush);

        let data = coalescer.flush();
        assert_eq!(data, b"hello world");
        assert!(coalescer.is_empty());
    }

    #[test]
    fn test_padding_mode_parse() {
        assert_eq!(PaddingMode::parse("bucket"), PaddingMode::Bucket);
        assert_eq!(PaddingMode::parse("Bucket"), PaddingMode::Bucket);
        assert_eq!(PaddingMode::parse("random"), PaddingMode::Random);
        assert_eq!(PaddingMode::parse("none"), PaddingMode::None);
        assert_eq!(PaddingMode::parse("invalid"), PaddingMode::None);
    }
}
