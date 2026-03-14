//! Padding extension beacon — auth tag generation and verification.
//!
//! Client flow:
//! 1. Compute epoch = floor(unix_time / rotation_interval)
//! 2. Compute auth_tag = BLAKE3("prisma-auth", master_secret | ephemeral_pub | epoch)\[0..16\]
//! 3. Compute position = BLAKE3("prisma-auth-pos", master_secret | epoch) % (padding_len - 16)
//! 4. Fill padding extension with rand() bytes, insert auth_tag at position
//!
//! Server flow:
//! 1. Extract padding extension from received ClientHello
//! 2. For each registered client's master_secret, for each epoch in {epoch-1, epoch, epoch+1}:
//!    - Compute expected position and extract 16 bytes at that offset
//!    - Compute expected auth_tag using the client's key_share public key from ClientHello
//!    - If match -> authenticated PrismaVeil client
//! 3. If no match -> probe/browser -> relay to mask server

use subtle::ConstantTimeEq;

/// Length of the authentication tag in bytes.
const AUTH_TAG_LEN: usize = 16;

/// Domain separation string for auth tag derivation.
const AUTH_TAG_DOMAIN: &str = "prisma-auth";

/// Domain separation string for tag position derivation.
const AUTH_POS_DOMAIN: &str = "prisma-auth-pos";

/// Generate a 16-byte authentication tag.
///
/// Uses BLAKE3 keyed hash with domain separation:
/// `BLAKE3("prisma-auth", master_secret | ephemeral_pub | epoch)[0..16]`
pub fn generate_auth_tag(
    master_secret: &[u8; 32],
    ephemeral_pub: &[u8; 32],
    epoch: u64,
) -> [u8; 16] {
    // Derive a 32-byte key from the domain string for BLAKE3 keyed hashing.
    let key = blake3::derive_key(AUTH_TAG_DOMAIN, master_secret);

    let mut hasher = blake3::Hasher::new_keyed(&key);
    hasher.update(master_secret);
    hasher.update(ephemeral_pub);
    hasher.update(&epoch.to_le_bytes());

    let hash = hasher.finalize();
    let mut tag = [0u8; AUTH_TAG_LEN];
    tag.copy_from_slice(&hash.as_bytes()[..AUTH_TAG_LEN]);
    tag
}

/// Compute the position within the padding where the auth tag should be placed.
///
/// Uses BLAKE3: `BLAKE3("prisma-auth-pos", master_secret | epoch) % (padding_len - 16)`
///
/// # Panics
///
/// Panics if `padding_len < AUTH_TAG_LEN` (the padding must be large enough to hold the tag).
pub fn compute_tag_position(master_secret: &[u8; 32], epoch: u64, padding_len: usize) -> usize {
    assert!(
        padding_len >= AUTH_TAG_LEN,
        "padding_len ({padding_len}) must be >= {AUTH_TAG_LEN}"
    );

    let key = blake3::derive_key(AUTH_POS_DOMAIN, master_secret);

    let mut hasher = blake3::Hasher::new_keyed(&key);
    hasher.update(master_secret);
    hasher.update(&epoch.to_le_bytes());

    let hash = hasher.finalize();
    let hash_bytes = hash.as_bytes();

    // Interpret the first 8 bytes as a u64 for the modulo operation.
    let val = u64::from_le_bytes(hash_bytes[..8].try_into().unwrap());
    let range = (padding_len - AUTH_TAG_LEN) as u64 + 1;
    (val % range) as usize
}

/// Verify an auth tag hidden in padding data.
///
/// Computes the expected position, extracts 16 bytes, and compares with the
/// expected tag using constant-time comparison.
///
/// Returns `true` if the padding contains a valid auth tag for the given parameters.
pub fn verify_auth_tag(
    padding_data: &[u8],
    ephemeral_pub: &[u8; 32],
    master_secret: &[u8; 32],
    epoch: u64,
) -> bool {
    let padding_len = padding_data.len();
    if padding_len < AUTH_TAG_LEN {
        return false;
    }

    let position = compute_tag_position(master_secret, epoch, padding_len);
    let end = position + AUTH_TAG_LEN;
    if end > padding_len {
        return false;
    }

    let expected_tag = generate_auth_tag(master_secret, ephemeral_pub, epoch);
    let candidate = &padding_data[position..end];

    // Constant-time comparison to prevent timing side-channels.
    candidate.ct_eq(&expected_tag).into()
}

/// Build a full padding extension content with the auth tag hidden inside random bytes.
///
/// The resulting `Vec<u8>` has exactly `padding_len` bytes, filled with random data
/// except for the 16-byte auth tag at the derived position.
///
/// # Panics
///
/// Panics if `padding_len < AUTH_TAG_LEN`.
pub fn build_auth_padding(
    master_secret: &[u8; 32],
    ephemeral_pub: &[u8; 32],
    epoch: u64,
    padding_len: usize,
) -> Vec<u8> {
    assert!(
        padding_len >= AUTH_TAG_LEN,
        "padding_len ({padding_len}) must be >= {AUTH_TAG_LEN}"
    );

    use rand::RngCore;

    // Fill the entire padding with random bytes.
    let mut padding = vec![0u8; padding_len];
    rand::thread_rng().fill_bytes(&mut padding);

    // Compute position and auth tag, then overwrite at that position.
    let position = compute_tag_position(master_secret, epoch, padding_len);
    let tag = generate_auth_tag(master_secret, ephemeral_pub, epoch);
    padding[position..position + AUTH_TAG_LEN].copy_from_slice(&tag);

    padding
}

/// Server-side multi-epoch, multi-client verification.
///
/// Tries all `master_secrets` across the epoch range `[current_epoch - allowed_skew ..=
/// current_epoch + allowed_skew]`. Returns the index of the first matching master secret,
/// or `None` if no match is found.
///
/// The `allowed_skew` parameter specifies how many epochs on either side of the current
/// epoch to check, accommodating clock drift between client and server.
pub fn verify_padding_multi_epoch(
    padding_data: &[u8],
    ephemeral_pub: &[u8; 32],
    master_secrets: &[[u8; 32]],
    allowed_skew: u8,
) -> Option<usize> {
    use crate::prisma_auth::rotation::current_epoch;

    // Use a 1-hour default rotation interval for the epoch calculation.
    // In production, this should come from the server config.
    let rotation_interval_secs = 3600u64;
    let now_epoch = current_epoch(rotation_interval_secs);

    let skew = allowed_skew as u64;
    let start_epoch = now_epoch.saturating_sub(skew);
    let end_epoch = now_epoch.saturating_add(skew);

    for (client_index, secret) in master_secrets.iter().enumerate() {
        for epoch in start_epoch..=end_epoch {
            if verify_auth_tag(padding_data, ephemeral_pub, secret, epoch) {
                return Some(client_index);
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_secret() -> [u8; 32] {
        let mut s = [0u8; 32];
        s[0] = 0xAA;
        s[31] = 0xBB;
        s
    }

    fn test_ephemeral() -> [u8; 32] {
        let mut e = [0u8; 32];
        e[0] = 0xCC;
        e[15] = 0xDD;
        e
    }

    #[test]
    fn generate_auth_tag_deterministic() {
        let secret = test_secret();
        let eph = test_ephemeral();
        let epoch = 1000u64;

        let tag1 = generate_auth_tag(&secret, &eph, epoch);
        let tag2 = generate_auth_tag(&secret, &eph, epoch);
        assert_eq!(tag1, tag2, "Same inputs must produce the same tag");
    }

    #[test]
    fn generate_auth_tag_different_epochs_differ() {
        let secret = test_secret();
        let eph = test_ephemeral();

        let tag_a = generate_auth_tag(&secret, &eph, 100);
        let tag_b = generate_auth_tag(&secret, &eph, 101);
        assert_ne!(tag_a, tag_b, "Different epochs must produce different tags");
    }

    #[test]
    fn generate_auth_tag_different_secrets_differ() {
        let secret_a = test_secret();
        let mut secret_b = test_secret();
        secret_b[5] = 0xFF;
        let eph = test_ephemeral();

        let tag_a = generate_auth_tag(&secret_a, &eph, 42);
        let tag_b = generate_auth_tag(&secret_b, &eph, 42);
        assert_ne!(
            tag_a, tag_b,
            "Different secrets must produce different tags"
        );
    }

    #[test]
    fn generate_auth_tag_different_ephemeral_differ() {
        let secret = test_secret();
        let eph_a = test_ephemeral();
        let mut eph_b = test_ephemeral();
        eph_b[10] = 0xEE;

        let tag_a = generate_auth_tag(&secret, &eph_a, 42);
        let tag_b = generate_auth_tag(&secret, &eph_b, 42);
        assert_ne!(
            tag_a, tag_b,
            "Different ephemeral keys must produce different tags"
        );
    }

    #[test]
    fn compute_tag_position_within_bounds() {
        let secret = test_secret();
        let padding_len = 200usize;

        for epoch in 0..100 {
            let pos = compute_tag_position(&secret, epoch, padding_len);
            assert!(
                pos + AUTH_TAG_LEN <= padding_len,
                "Position {pos} + tag length {AUTH_TAG_LEN} exceeds padding_len {padding_len} at epoch {epoch}"
            );
        }
    }

    #[test]
    fn compute_tag_position_deterministic() {
        let secret = test_secret();
        let pos1 = compute_tag_position(&secret, 500, 300);
        let pos2 = compute_tag_position(&secret, 500, 300);
        assert_eq!(pos1, pos2);
    }

    #[test]
    fn compute_tag_position_minimum_padding() {
        let secret = test_secret();
        // When padding_len == AUTH_TAG_LEN, the only valid position is 0.
        let pos = compute_tag_position(&secret, 42, AUTH_TAG_LEN);
        assert_eq!(
            pos, 0,
            "With padding_len == AUTH_TAG_LEN, position must be 0"
        );
    }

    #[test]
    #[should_panic(expected = "padding_len")]
    fn compute_tag_position_too_small_panics() {
        let secret = test_secret();
        compute_tag_position(&secret, 0, AUTH_TAG_LEN - 1);
    }

    #[test]
    fn verify_auth_tag_roundtrip() {
        let secret = test_secret();
        let eph = test_ephemeral();
        let epoch = 777u64;
        let padding_len = 256usize;

        let padding = build_auth_padding(&secret, &eph, epoch, padding_len);
        assert_eq!(padding.len(), padding_len);

        assert!(
            verify_auth_tag(&padding, &eph, &secret, epoch),
            "Verification must succeed for correctly built padding"
        );
    }

    #[test]
    fn verify_auth_tag_wrong_epoch_fails() {
        let secret = test_secret();
        let eph = test_ephemeral();
        let epoch = 777u64;
        let padding_len = 256usize;

        let padding = build_auth_padding(&secret, &eph, epoch, padding_len);

        assert!(
            !verify_auth_tag(&padding, &eph, &secret, epoch + 1),
            "Verification must fail for wrong epoch"
        );
    }

    #[test]
    fn verify_auth_tag_wrong_secret_fails() {
        let secret = test_secret();
        let eph = test_ephemeral();
        let epoch = 100u64;
        let padding_len = 256usize;

        let padding = build_auth_padding(&secret, &eph, epoch, padding_len);

        let mut wrong_secret = secret;
        wrong_secret[0] ^= 0xFF;
        assert!(
            !verify_auth_tag(&padding, &eph, &wrong_secret, epoch),
            "Verification must fail for wrong secret"
        );
    }

    #[test]
    fn verify_auth_tag_wrong_ephemeral_fails() {
        let secret = test_secret();
        let eph = test_ephemeral();
        let epoch = 100u64;
        let padding_len = 256usize;

        let padding = build_auth_padding(&secret, &eph, epoch, padding_len);

        let mut wrong_eph = eph;
        wrong_eph[0] ^= 0xFF;
        assert!(
            !verify_auth_tag(&padding, &wrong_eph, &secret, epoch),
            "Verification must fail for wrong ephemeral key"
        );
    }

    #[test]
    fn verify_auth_tag_corrupted_padding_fails() {
        let secret = test_secret();
        let eph = test_ephemeral();
        let epoch = 55u64;
        let padding_len = 256usize;

        let mut padding = build_auth_padding(&secret, &eph, epoch, padding_len);

        // Corrupt the byte at the tag position.
        let pos = compute_tag_position(&secret, epoch, padding_len);
        padding[pos] ^= 0x01;

        assert!(
            !verify_auth_tag(&padding, &eph, &secret, epoch),
            "Verification must fail when padding is corrupted at the tag position"
        );
    }

    #[test]
    fn verify_auth_tag_padding_too_short() {
        let secret = test_secret();
        let eph = test_ephemeral();
        let short_padding = vec![0u8; AUTH_TAG_LEN - 1];
        assert!(
            !verify_auth_tag(&short_padding, &eph, &secret, 0),
            "Verification must return false for padding shorter than AUTH_TAG_LEN"
        );
    }

    #[test]
    fn build_auth_padding_length_correct() {
        let secret = test_secret();
        let eph = test_ephemeral();

        for len in [16, 32, 64, 128, 256, 512] {
            let padding = build_auth_padding(&secret, &eph, 0, len);
            assert_eq!(padding.len(), len);
        }
    }

    #[test]
    fn build_auth_padding_not_all_zeros() {
        let secret = test_secret();
        let eph = test_ephemeral();
        let padding = build_auth_padding(&secret, &eph, 0, 512);

        // The padding should have random bytes — extremely unlikely to be all zeros.
        let all_zero = padding.iter().all(|&b| b == 0);
        assert!(
            !all_zero,
            "Padding should contain random bytes, not all zeros"
        );
    }

    #[test]
    #[should_panic(expected = "padding_len")]
    fn build_auth_padding_too_small_panics() {
        let secret = test_secret();
        let eph = test_ephemeral();
        build_auth_padding(&secret, &eph, 0, AUTH_TAG_LEN - 1);
    }

    #[test]
    fn verify_padding_multi_epoch_finds_correct_client() {
        let secret_0 = [0x11u8; 32];
        let secret_1 = test_secret();
        let secret_2 = [0x22u8; 32];
        let eph = test_ephemeral();

        let secrets = [secret_0, secret_1, secret_2];

        // Build padding using secret_1 (index 1) and the current epoch.
        let rotation_interval = 3600u64;
        let epoch = crate::prisma_auth::rotation::current_epoch(rotation_interval);
        let padding = build_auth_padding(&secret_1, &eph, epoch, 256);

        let result = verify_padding_multi_epoch(&padding, &eph, &secrets, 1);
        assert_eq!(result, Some(1), "Should find client at index 1");
    }

    #[test]
    fn verify_padding_multi_epoch_no_match() {
        let secret = test_secret();
        let eph = test_ephemeral();

        // Build padding with one secret but verify against different secrets.
        let rotation_interval = 3600u64;
        let epoch = crate::prisma_auth::rotation::current_epoch(rotation_interval);
        let padding = build_auth_padding(&secret, &eph, epoch, 256);

        let other_secrets = [[0xFFu8; 32], [0xEEu8; 32]];
        let result = verify_padding_multi_epoch(&padding, &eph, &other_secrets, 1);
        assert_eq!(result, None, "Should return None when no secret matches");
    }

    #[test]
    fn verify_padding_multi_epoch_adjacent_epoch() {
        let secret = test_secret();
        let eph = test_ephemeral();

        // Build padding with epoch - 1 to simulate slight clock skew.
        let rotation_interval = 3600u64;
        let epoch = crate::prisma_auth::rotation::current_epoch(rotation_interval);
        let prev_epoch = epoch.saturating_sub(1);
        let padding = build_auth_padding(&secret, &eph, prev_epoch, 256);

        let secrets = [secret];
        let result = verify_padding_multi_epoch(&padding, &eph, &secrets, 1);
        assert_eq!(
            result,
            Some(0),
            "Should match when padding was built with epoch-1 and skew=1"
        );
    }
}
