//! Entropy camouflage — byte distribution shaping for GFW exemption.
//!
//! The GFW uses entropy-based heuristics to classify encrypted traffic.
//! Traffic is exempted (allowed) if it matches ANY of these rules:
//!
//! - **Ex1**: Byte popcount ≤ 3.4 or ≥ 4.6 (encrypted data clusters ~4.0)
//! - **Ex2**: First 6+ bytes are printable ASCII
//! - **Ex5**: Matches TLS signature `[\x16-\x17]\x03[\x00-\x09]`
//!
//! This module provides utilities to shape the first packet to pass these rules.

use rand::Rng;

/// ASCII prefix bytes to prepend to Salamander/raw UDP packets.
/// Passes GFW Ex2 rule (first 6+ bytes are printable ASCII).
pub const ASCII_PREFIX_LEN: usize = 8;

/// Generate a random ASCII prefix that looks like a plausible protocol header.
/// All bytes are in the printable ASCII range (0x20-0x7E).
pub fn generate_ascii_prefix() -> [u8; ASCII_PREFIX_LEN] {
    let mut rng = rand::thread_rng();
    let mut prefix = [0u8; ASCII_PREFIX_LEN];
    for byte in prefix.iter_mut() {
        *byte = rng.gen_range(0x20..=0x7E);
    }
    prefix
}

/// Check whether a byte sequence has an ASCII prefix of length >= `min_len`.
pub fn has_ascii_prefix(data: &[u8], min_len: usize) -> bool {
    if data.len() < min_len {
        return false;
    }
    data[..min_len].iter().all(|&b| (0x20..=0x7E).contains(&b))
}

/// Compute the average popcount (number of 1-bits per byte) of a byte slice.
pub fn average_popcount(data: &[u8]) -> f64 {
    if data.is_empty() {
        return 0.0;
    }
    let total_ones: u32 = data.iter().map(|b| b.count_ones()).sum();
    total_ones as f64 / data.len() as f64
}

/// Shape a buffer's byte distribution so the average popcount falls outside
/// the suspicious range [3.4, 4.6] (i.e., passes GFW Ex1 rule).
///
/// Strategy: Append biased padding bytes that push the overall popcount
/// above 4.6 (use bytes with many 1-bits, like 0xFF) or below 3.4
/// (use bytes with few 1-bits, like 0x00).
///
/// Returns the padding bytes to append.
pub fn shape_entropy_padding(data: &[u8], target_direction: PopcountTarget) -> Vec<u8> {
    let current = average_popcount(data);

    // If already outside suspicious range, no padding needed
    if current <= 3.4 || current >= 4.6 {
        return Vec::new();
    }

    let data_len = data.len();
    let (bias_byte, target_popcount) = match target_direction {
        PopcountTarget::Low => (0x01u8, 3.0), // 1 bit per byte → pushes popcount down
        PopcountTarget::High => (0xFEu8, 5.0), // 7 bits per byte → pushes popcount up
    };

    let bias_bits = bias_byte.count_ones() as f64;

    // Calculate how many padding bytes we need:
    // (data_len * current + pad_len * bias_bits) / (data_len + pad_len) = target
    // Solving: pad_len = data_len * (current - target) / (target - bias_bits)
    let numerator = data_len as f64 * (current - target_popcount).abs();
    let denominator = (target_popcount - bias_bits).abs();

    if denominator < 0.001 {
        return Vec::new();
    }

    let pad_len = (numerator / denominator).ceil() as usize;
    // Cap padding to avoid excessively large packets
    let pad_len = pad_len.min(256);

    vec![bias_byte; pad_len]
}

/// Direction to bias the popcount toward.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopcountTarget {
    /// Push popcount below 3.4
    Low,
    /// Push popcount above 4.6
    High,
}

impl Default for PopcountTarget {
    fn default() -> Self {
        PopcountTarget::High
    }
}

/// Check if a packet matches the TLS record signature (passes GFW Ex5).
/// TLS record: `[\x16-\x17]\x03[\x00-\x09]`
pub fn looks_like_tls_record(data: &[u8]) -> bool {
    if data.len() < 3 {
        return false;
    }
    (data[0] == 0x16 || data[0] == 0x17) && data[1] == 0x03 && data[2] <= 0x09
}

/// Check if a packet would pass any GFW exemption rule.
pub fn passes_gfw_exemption(data: &[u8]) -> bool {
    if data.is_empty() {
        return false;
    }

    // Ex1: popcount outside [3.4, 4.6]
    let popcount = average_popcount(data);
    if popcount <= 3.4 || popcount >= 4.6 {
        return true;
    }

    // Ex2: first 6+ bytes are printable ASCII
    if has_ascii_prefix(data, 6) {
        return true;
    }

    // Ex5: TLS record signature
    if looks_like_tls_record(data) {
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ascii_prefix_generation() {
        let prefix = generate_ascii_prefix();
        assert_eq!(prefix.len(), ASCII_PREFIX_LEN);
        for &b in &prefix {
            assert!(
                b >= 0x20 && b <= 0x7E,
                "byte {:#04x} not printable ASCII",
                b
            );
        }
    }

    #[test]
    fn test_has_ascii_prefix() {
        assert!(has_ascii_prefix(b"Hello World!", 6));
        assert!(!has_ascii_prefix(b"\x00\x01Hello", 6));
        assert!(!has_ascii_prefix(b"Hi", 6));
    }

    #[test]
    fn test_average_popcount() {
        // 0xFF has popcount 8, 0x00 has popcount 0
        assert!((average_popcount(&[0xFF]) - 8.0).abs() < 0.001);
        assert!((average_popcount(&[0x00]) - 0.0).abs() < 0.001);
        // 0xAA (10101010) has popcount 4
        assert!((average_popcount(&[0xAA]) - 4.0).abs() < 0.001);
    }

    #[test]
    fn test_entropy_shaping_high() {
        // Random-looking data with popcount ~4.0 (suspicious range)
        let data = vec![0xAA; 100]; // popcount exactly 4.0
        let padding = shape_entropy_padding(&data, PopcountTarget::High);
        assert!(!padding.is_empty());

        let mut combined = data.clone();
        combined.extend_from_slice(&padding);
        let new_popcount = average_popcount(&combined);
        assert!(
            new_popcount >= 4.6 || new_popcount <= 3.4,
            "popcount {} still in suspicious range",
            new_popcount
        );
    }

    #[test]
    fn test_entropy_shaping_not_needed() {
        // Data already outside suspicious range
        let data = vec![0xFF; 100]; // popcount 8.0
        let padding = shape_entropy_padding(&data, PopcountTarget::High);
        assert!(padding.is_empty());
    }

    #[test]
    fn test_tls_record_detection() {
        assert!(looks_like_tls_record(&[0x16, 0x03, 0x01])); // TLS Handshake
        assert!(looks_like_tls_record(&[0x17, 0x03, 0x03])); // TLS Application Data
        assert!(!looks_like_tls_record(&[0x15, 0x03, 0x01])); // Not in range
        assert!(!looks_like_tls_record(&[0x16, 0x04, 0x01])); // Wrong major version
    }

    #[test]
    fn test_passes_gfw_exemption() {
        // TLS record → passes
        assert!(passes_gfw_exemption(&[0x16, 0x03, 0x01, 0x00, 0x00]));
        // ASCII prefix → passes
        assert!(passes_gfw_exemption(b"GET / HTTP/1.1\r\n"));
        // High popcount → passes
        assert!(passes_gfw_exemption(&[0xFF; 10]));
        // Random encrypted-looking data → fails (popcount ~4.0, not ASCII, not TLS)
        // Use 0x90 (10010000) which has popcount 2 but with non-ASCII first bytes
        // Actually, we need bytes that: popcount in [3.4, 4.6], not ASCII (>0x7E), not TLS
        // 0x88 (10001000) popcount=2 → NOT in suspicious range (≤3.4), so it passes Ex1
        // We need something with popcount exactly ~4.0 and first byte > 0x7E
        // 0xA5 (10100101) = popcount 4, non-ASCII, not TLS header
        let mut data = vec![0xA5; 100];
        data[0] = 0xA5; // Non-ASCII, non-TLS
        assert!(!passes_gfw_exemption(&data));
    }
}
