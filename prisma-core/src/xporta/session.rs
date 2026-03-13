use crate::util::hex_encode;

/// Derive the server cookie key from the first auth secret.
/// `server_cookie_key = BLAKE3_derive_key("prisma-xporta-cookie-v1", first_auth_secret)`
pub fn derive_cookie_key(first_auth_secret: &[u8; 32]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new_derive_key("prisma-xporta-cookie-v1");
    hasher.update(first_auth_secret);
    let mut key = [0u8; 32];
    let mut reader = hasher.finalize_xof();
    reader.fill(&mut key);
    key
}

/// Create a cookie token for an XPorta session.
///
/// Token = hex(session_id[16] || expiry_le[8] || BLAKE3_keyed(cookie_key, session_id || client_id || expiry))
///
/// Returns the hex-encoded token string.
pub fn create_cookie_token(
    cookie_key: &[u8; 32],
    session_id: &[u8; 16],
    client_id: &[u8; 16],
    expiry: u64,
) -> String {
    let mac = compute_cookie_mac(cookie_key, session_id, client_id, expiry);

    // session_id(16) || expiry(8) || mac(32) = 56 bytes
    let mut token = Vec::with_capacity(56);
    token.extend_from_slice(session_id);
    token.extend_from_slice(&expiry.to_le_bytes());
    token.extend_from_slice(&mac);

    hex_encode(&token)
}

/// Verify a cookie token and extract session_id if valid.
///
/// Returns `Some((session_id, expiry))` if the token is valid and not expired.
pub fn verify_cookie_token(
    cookie_key: &[u8; 32],
    token_hex: &str,
    client_id: &[u8; 16],
    now: u64,
) -> Option<([u8; 16], u64)> {
    let token = crate::util::hex_decode(token_hex)?;
    if token.len() != 56 {
        return None;
    }

    let mut session_id = [0u8; 16];
    session_id.copy_from_slice(&token[0..16]);
    let expiry = u64::from_le_bytes(token[16..24].try_into().ok()?);
    let provided_mac: [u8; 32] = token[24..56].try_into().ok()?;

    // Check expiry
    if now > expiry {
        return None;
    }

    // Compute expected MAC
    let expected_mac = compute_cookie_mac(cookie_key, &session_id, client_id, expiry);

    // Constant-time comparison
    if !crate::util::ct_eq(&provided_mac, &expected_mac) {
        return None;
    }

    Some((session_id, expiry))
}

/// Compute BLAKE3 keyed MAC for the cookie.
fn compute_cookie_mac(
    cookie_key: &[u8; 32],
    session_id: &[u8; 16],
    client_id: &[u8; 16],
    expiry: u64,
) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new_keyed(cookie_key);
    hasher.update(session_id);
    hasher.update(client_id);
    hasher.update(&expiry.to_le_bytes());
    *hasher.finalize().as_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cookie_roundtrip() {
        let auth_secret = [0x42u8; 32];
        let cookie_key = derive_cookie_key(&auth_secret);

        let session_id = [1u8; 16];
        let client_id = [2u8; 16];
        let expiry = 1700000300; // 300 seconds from some base

        let token = create_cookie_token(&cookie_key, &session_id, &client_id, expiry);

        // Verify with time before expiry
        let result = verify_cookie_token(&cookie_key, &token, &client_id, 1700000000);
        assert!(result.is_some());
        let (sid, exp) = result.unwrap();
        assert_eq!(sid, session_id);
        assert_eq!(exp, expiry);
    }

    #[test]
    fn test_cookie_expired() {
        let auth_secret = [0x42u8; 32];
        let cookie_key = derive_cookie_key(&auth_secret);

        let session_id = [1u8; 16];
        let client_id = [2u8; 16];
        let expiry = 1700000300;

        let token = create_cookie_token(&cookie_key, &session_id, &client_id, expiry);

        // Verify with time after expiry
        let result = verify_cookie_token(&cookie_key, &token, &client_id, 1700000301);
        assert!(result.is_none());
    }

    #[test]
    fn test_cookie_wrong_client() {
        let auth_secret = [0x42u8; 32];
        let cookie_key = derive_cookie_key(&auth_secret);

        let session_id = [1u8; 16];
        let client_id = [2u8; 16];
        let wrong_client_id = [3u8; 16];
        let expiry = 1700000300;

        let token = create_cookie_token(&cookie_key, &session_id, &client_id, expiry);

        // Verify with wrong client_id
        let result = verify_cookie_token(&cookie_key, &token, &wrong_client_id, 1700000000);
        assert!(result.is_none());
    }

    #[test]
    fn test_cookie_tampered() {
        let auth_secret = [0x42u8; 32];
        let cookie_key = derive_cookie_key(&auth_secret);

        let session_id = [1u8; 16];
        let client_id = [2u8; 16];
        let expiry = 1700000300;

        let mut token = create_cookie_token(&cookie_key, &session_id, &client_id, expiry);

        // Tamper with the token (flip a character)
        let mut chars: Vec<char> = token.chars().collect();
        let last = chars.len() - 1;
        chars[last] = if chars[last] == '0' { '1' } else { '0' };
        token = chars.into_iter().collect();

        let result = verify_cookie_token(&cookie_key, &token, &client_id, 1700000000);
        assert!(result.is_none());
    }

    #[test]
    fn test_cookie_invalid_hex() {
        let auth_secret = [0x42u8; 32];
        let cookie_key = derive_cookie_key(&auth_secret);
        let client_id = [2u8; 16];

        let result = verify_cookie_token(&cookie_key, "not-valid-hex!", &client_id, 1700000000);
        assert!(result.is_none());
    }

    #[test]
    fn test_cookie_wrong_length() {
        let auth_secret = [0x42u8; 32];
        let cookie_key = derive_cookie_key(&auth_secret);
        let client_id = [2u8; 16];

        let result = verify_cookie_token(&cookie_key, "aabbccdd", &client_id, 1700000000);
        assert!(result.is_none());
    }

    #[test]
    fn test_derive_cookie_key_deterministic() {
        let secret = [0xAB; 32];
        let key1 = derive_cookie_key(&secret);
        let key2 = derive_cookie_key(&secret);
        assert_eq!(key1, key2);

        // Different secret = different key
        let secret2 = [0xCD; 32];
        let key3 = derive_cookie_key(&secret2);
        assert_ne!(key1, key3);
    }
}
