/// Derive a session key from the shared secret and contextual binding data.
///
/// Uses BLAKE3's key derivation mode with a domain separation string.
/// The context includes both public keys and a timestamp to ensure
/// unique keys per session even if ephemeral keys are somehow reused.
pub fn derive_session_key(
    shared_secret: &[u8; 32],
    client_pub: &[u8; 32],
    server_pub: &[u8; 32],
    timestamp: u64,
) -> [u8; 32] {
    let mut context = Vec::with_capacity(32 + 32 + 32 + 8);
    context.extend_from_slice(shared_secret);
    context.extend_from_slice(client_pub);
    context.extend_from_slice(server_pub);
    context.extend_from_slice(&timestamp.to_be_bytes());

    let mut output = [0u8; 32];
    let mut hasher = blake3::Hasher::new_derive_key("prisma-veil-v1-session-key");
    hasher.update(&context);
    let mut reader = hasher.finalize_xof();
    reader.fill(&mut output);
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kdf_determinism() {
        let secret = [0xABu8; 32];
        let client_pub = [0x01u8; 32];
        let server_pub = [0x02u8; 32];
        let timestamp = 1234567890u64;

        let key1 = derive_session_key(&secret, &client_pub, &server_pub, timestamp);
        let key2 = derive_session_key(&secret, &client_pub, &server_pub, timestamp);

        assert_eq!(key1, key2);
    }

    #[test]
    fn test_kdf_different_inputs_different_keys() {
        let secret = [0xABu8; 32];
        let client_pub = [0x01u8; 32];
        let server_pub = [0x02u8; 32];

        let key1 = derive_session_key(&secret, &client_pub, &server_pub, 1000);
        let key2 = derive_session_key(&secret, &client_pub, &server_pub, 1001);

        assert_ne!(key1, key2);
    }

    #[test]
    fn test_kdf_key_length() {
        let secret = [0u8; 32];
        let client_pub = [0u8; 32];
        let server_pub = [0u8; 32];

        let key = derive_session_key(&secret, &client_pub, &server_pub, 0);
        assert_eq!(key.len(), 32);
        // Should not be all zeros (vanishingly unlikely)
        assert_ne!(key, [0u8; 32]);
    }
}
