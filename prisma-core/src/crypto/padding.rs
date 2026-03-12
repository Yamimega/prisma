use rand::Rng;

use crate::types::MAX_PADDING_SIZE;

/// Generate random padding of random length up to `max_size`.
/// Format: [padding_len:2][random_bytes:padding_len]
/// The first 2 bytes encode the length so the receiver can strip it.
pub fn generate_padding(max_size: usize) -> Vec<u8> {
    let max = max_size.min(MAX_PADDING_SIZE);
    if max < 3 {
        return vec![0, 0]; // No room for padding data
    }
    let mut rng = rand::thread_rng();
    let padding_len = rng.gen_range(0..=(max - 2));
    let mut result = Vec::with_capacity(2 + padding_len);
    result.extend_from_slice(&(padding_len as u16).to_be_bytes());
    let mut buf = vec![0u8; padding_len];
    rng.fill(&mut buf[..]);
    result.extend_from_slice(&buf);
    result
}

/// Strip padding from the end of data. The last segment of `data` should be
/// the padding as produced by `generate_padding`. `padding_offset` is the
/// byte position where the padding begins.
///
/// Returns the data slice before the padding.
pub fn strip_padding(data: &[u8], padding_offset: usize) -> &[u8] {
    if padding_offset > data.len() {
        return data;
    }
    &data[..padding_offset]
}

/// Read the padding length from a padding block.
/// Returns the total size of the padding block (2-byte header + padding bytes).
pub fn read_padding_size(padding_block: &[u8]) -> Option<usize> {
    if padding_block.len() < 2 {
        return None;
    }
    let len = u16::from_be_bytes([padding_block[0], padding_block[1]]) as usize;
    Some(2 + len)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_padding_round_trip() {
        let padding = generate_padding(64);
        assert!(padding.len() >= 2);
        let total = read_padding_size(&padding).unwrap();
        assert_eq!(total, padding.len());
    }

    #[test]
    fn test_padding_max_size() {
        for _ in 0..100 {
            let padding = generate_padding(32);
            assert!(padding.len() <= 34); // 2 header + up to 32
        }
    }

    #[test]
    fn test_strip_padding() {
        let payload = b"hello";
        let padding = generate_padding(16);
        let mut data = Vec::new();
        data.extend_from_slice(payload);
        data.extend_from_slice(&padding);

        let stripped = strip_padding(&data, payload.len());
        assert_eq!(stripped, payload);
    }

    #[test]
    fn test_zero_max_padding() {
        let padding = generate_padding(0);
        assert_eq!(padding, vec![0, 0]);
    }
}
