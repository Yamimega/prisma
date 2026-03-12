use rand::rngs::OsRng;
use x25519_dalek::{EphemeralSecret, PublicKey, SharedSecret};

/// Ephemeral X25519 key pair for Diffie-Hellman key exchange.
pub struct EphemeralKeyPair {
    secret: EphemeralSecret,
    public: PublicKey,
}

impl EphemeralKeyPair {
    /// Generate a new ephemeral key pair using OS randomness.
    pub fn generate() -> Self {
        let secret = EphemeralSecret::random_from_rng(OsRng);
        let public = PublicKey::from(&secret);
        Self { secret, public }
    }

    /// Get the public key bytes.
    pub fn public_key(&self) -> &PublicKey {
        &self.public
    }

    /// Get the public key as raw bytes.
    pub fn public_key_bytes(&self) -> [u8; 32] {
        self.public.to_bytes()
    }

    /// Perform Diffie-Hellman key exchange, consuming the secret key.
    /// Returns the 32-byte shared secret.
    pub fn diffie_hellman(self, their_public: &PublicKey) -> [u8; 32] {
        let shared: SharedSecret = self.secret.diffie_hellman(their_public);
        *shared.as_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ecdh_shared_secret_agreement() {
        let alice = EphemeralKeyPair::generate();
        let bob = EphemeralKeyPair::generate();

        let alice_pub = *alice.public_key();
        let bob_pub = *bob.public_key();

        let alice_shared = alice.diffie_hellman(&bob_pub);
        let bob_shared = bob.diffie_hellman(&alice_pub);

        assert_eq!(alice_shared, bob_shared);
    }

    #[test]
    fn test_different_keys_different_secrets() {
        let alice = EphemeralKeyPair::generate();
        let bob = EphemeralKeyPair::generate();
        let carol = EphemeralKeyPair::generate();

        let bob_pub = *bob.public_key();
        let _carol_pub = *carol.public_key();

        let ab_shared = alice.diffie_hellman(&bob_pub);
        let cb_shared = carol.diffie_hellman(&bob_pub);

        // Extremely unlikely to be equal
        assert_ne!(ab_shared, cb_shared);
    }
}
