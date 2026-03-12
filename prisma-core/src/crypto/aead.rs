use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce as AesNonce};
use chacha20poly1305::{ChaCha20Poly1305, Nonce as ChaChaNonce};

use crate::error::CryptoError;
use crate::types::{CipherSuite, NONCE_SIZE};

/// Trait for authenticated encryption with associated data.
pub trait AeadCipher: Send + Sync {
    /// Encrypt plaintext with the given nonce and optional associated data.
    fn encrypt(
        &self,
        nonce: &[u8; NONCE_SIZE],
        plaintext: &[u8],
        aad: &[u8],
    ) -> Result<Vec<u8>, CryptoError>;

    /// Decrypt ciphertext with the given nonce and optional associated data.
    fn decrypt(
        &self,
        nonce: &[u8; NONCE_SIZE],
        ciphertext: &[u8],
        aad: &[u8],
    ) -> Result<Vec<u8>, CryptoError>;
}

pub struct ChaCha20Poly1305Cipher {
    cipher: ChaCha20Poly1305,
}

impl ChaCha20Poly1305Cipher {
    pub fn new(key: &[u8; 32]) -> Self {
        Self {
            cipher: ChaCha20Poly1305::new(key.into()),
        }
    }
}

impl AeadCipher for ChaCha20Poly1305Cipher {
    fn encrypt(
        &self,
        nonce: &[u8; NONCE_SIZE],
        plaintext: &[u8],
        aad: &[u8],
    ) -> Result<Vec<u8>, CryptoError> {
        let nonce = ChaChaNonce::from_slice(nonce);
        self.cipher
            .encrypt(
                nonce,
                Payload {
                    msg: plaintext,
                    aad,
                },
            )
            .map_err(|e| CryptoError::EncryptionFailed(e.to_string()))
    }

    fn decrypt(
        &self,
        nonce: &[u8; NONCE_SIZE],
        ciphertext: &[u8],
        aad: &[u8],
    ) -> Result<Vec<u8>, CryptoError> {
        let nonce = ChaChaNonce::from_slice(nonce);
        self.cipher
            .decrypt(
                nonce,
                Payload {
                    msg: ciphertext,
                    aad,
                },
            )
            .map_err(|e| CryptoError::DecryptionFailed(e.to_string()))
    }
}

pub struct Aes256GcmCipher {
    cipher: Aes256Gcm,
}

impl Aes256GcmCipher {
    pub fn new(key: &[u8; 32]) -> Self {
        Self {
            cipher: Aes256Gcm::new(key.into()),
        }
    }
}

impl AeadCipher for Aes256GcmCipher {
    fn encrypt(
        &self,
        nonce: &[u8; NONCE_SIZE],
        plaintext: &[u8],
        aad: &[u8],
    ) -> Result<Vec<u8>, CryptoError> {
        let nonce = AesNonce::from_slice(nonce);
        self.cipher
            .encrypt(
                nonce,
                Payload {
                    msg: plaintext,
                    aad,
                },
            )
            .map_err(|e| CryptoError::EncryptionFailed(e.to_string()))
    }

    fn decrypt(
        &self,
        nonce: &[u8; NONCE_SIZE],
        ciphertext: &[u8],
        aad: &[u8],
    ) -> Result<Vec<u8>, CryptoError> {
        let nonce = AesNonce::from_slice(nonce);
        self.cipher
            .decrypt(
                nonce,
                Payload {
                    msg: ciphertext,
                    aad,
                },
            )
            .map_err(|e| CryptoError::DecryptionFailed(e.to_string()))
    }
}

/// Create a cipher instance for the given suite and key.
pub fn create_cipher(suite: CipherSuite, key: &[u8; 32]) -> Box<dyn AeadCipher> {
    match suite {
        CipherSuite::ChaCha20Poly1305 => Box::new(ChaCha20Poly1305Cipher::new(key)),
        CipherSuite::Aes256Gcm => Box::new(Aes256GcmCipher::new(key)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_round_trip(suite: CipherSuite) {
        let key = [0x42u8; 32];
        let cipher = create_cipher(suite, &key);
        let nonce = [0u8; NONCE_SIZE];
        let plaintext = b"hello, prisma!";
        let aad = b"session-1";

        let ciphertext = cipher.encrypt(&nonce, plaintext, aad).unwrap();
        let decrypted = cipher.decrypt(&nonce, &ciphertext, aad).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_chacha20_round_trip() {
        test_round_trip(CipherSuite::ChaCha20Poly1305);
    }

    #[test]
    fn test_aes256gcm_round_trip() {
        test_round_trip(CipherSuite::Aes256Gcm);
    }

    #[test]
    fn test_wrong_key_rejection() {
        let key1 = [0x42u8; 32];
        let key2 = [0x43u8; 32];
        let cipher1 = create_cipher(CipherSuite::ChaCha20Poly1305, &key1);
        let cipher2 = create_cipher(CipherSuite::ChaCha20Poly1305, &key2);

        let nonce = [0u8; NONCE_SIZE];
        let ciphertext = cipher1.encrypt(&nonce, b"secret", b"").unwrap();
        assert!(cipher2.decrypt(&nonce, &ciphertext, b"").is_err());
    }

    #[test]
    fn test_wrong_aad_rejection() {
        let key = [0x42u8; 32];
        let cipher = create_cipher(CipherSuite::Aes256Gcm, &key);
        let nonce = [0u8; NONCE_SIZE];

        let ciphertext = cipher.encrypt(&nonce, b"secret", b"correct-aad").unwrap();
        assert!(cipher.decrypt(&nonce, &ciphertext, b"wrong-aad").is_err());
    }

    #[test]
    fn test_ciphertext_differs_from_plaintext() {
        let key = [0x42u8; 32];
        let cipher = create_cipher(CipherSuite::ChaCha20Poly1305, &key);
        let nonce = [0u8; NONCE_SIZE];
        let plaintext = b"hello, prisma!";

        let ciphertext = cipher.encrypt(&nonce, plaintext, b"").unwrap();
        assert_ne!(&ciphertext[..plaintext.len()], &plaintext[..]);
    }
}
