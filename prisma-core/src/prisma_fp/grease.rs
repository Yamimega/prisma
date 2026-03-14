//! GREASE value generation matching Chrome's algorithm.
//!
//! GREASE (Generate Random Extensions And Sustain Extensibility) values
//! per RFC 8701 ensure that TLS implementations remain extensible by
//! injecting unknown-but-valid values into handshakes.

use rand::seq::SliceRandom;
use rand::Rng;

/// GREASE values defined in RFC 8701.
pub const GREASE_VALUES: &[u16] = &[
    0x0a0a, 0x1a1a, 0x2a2a, 0x3a3a, 0x4a4a, 0x5a5a, 0x6a6a, 0x7a7a, 0x8a8a, 0x9a9a, 0xaaaa, 0xbaba,
    0xcaca, 0xdada, 0xeaea, 0xfafa,
];

/// Generate a random GREASE value.
pub fn random_grease() -> u16 {
    let mut rng = rand::thread_rng();
    GREASE_VALUES[rng.gen_range(0..GREASE_VALUES.len())]
}

/// Generate N distinct GREASE values (Chrome uses different GREASE for different positions).
pub fn distinct_grease_values(n: usize) -> Vec<u16> {
    let mut values = GREASE_VALUES.to_vec();
    values.shuffle(&mut rand::thread_rng());
    values.into_iter().take(n).collect()
}

/// Check if a value is a GREASE value.
///
/// A value is GREASE if both bytes match the pattern 0x?a where the high
/// nibbles are equal: `(value & 0x0f0f) == 0x0a0a` and the two bytes share
/// the same high nibble.
pub fn is_grease(value: u16) -> bool {
    (value & 0x0f0f) == 0x0a0a
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_grease_values_detected() {
        for &v in GREASE_VALUES {
            assert!(is_grease(v), "Expected 0x{:04x} to be GREASE", v);
        }
    }

    #[test]
    fn test_non_grease_values_rejected() {
        assert!(!is_grease(0x0001));
        assert!(!is_grease(0x1301)); // TLS_AES_128_GCM_SHA256
        assert!(!is_grease(0xc02b));
        assert!(!is_grease(0x0000));
        assert!(!is_grease(0xffff));
    }

    #[test]
    fn test_random_grease_is_valid() {
        for _ in 0..100 {
            let v = random_grease();
            assert!(
                is_grease(v),
                "random_grease() returned non-GREASE 0x{:04x}",
                v
            );
        }
    }

    #[test]
    fn test_distinct_grease_values_are_distinct() {
        let values = distinct_grease_values(5);
        assert_eq!(values.len(), 5);

        // Check all distinct
        for i in 0..values.len() {
            for j in (i + 1)..values.len() {
                assert_ne!(values[i], values[j], "Values at {} and {} are equal", i, j);
            }
        }

        // Check all are valid GREASE
        for &v in &values {
            assert!(is_grease(v));
        }
    }

    #[test]
    fn test_distinct_grease_max_count() {
        // Can request up to 16 (total GREASE values available)
        let values = distinct_grease_values(16);
        assert_eq!(values.len(), 16);

        // Requesting more than 16 just returns 16
        let values = distinct_grease_values(20);
        assert_eq!(values.len(), 16);
    }
}
