use uuid::Uuid;

use crate::crypto::aead::create_cipher;
use crate::crypto::ecdh::EphemeralKeyPair;
use crate::crypto::kdf::derive_session_key;
use crate::error::{CryptoError, PrismaError, ProtocolError};
use crate::protocol::codec::*;
use crate::protocol::types::*;
use crate::types::{CipherSuite, ClientId, NONCE_SIZE, PROTOCOL_VERSION};
use crate::util;

/// Client-side handshake state machine.
pub struct ClientHandshake {
    client_id: ClientId,
    auth_secret: [u8; 32],
    preferred_cipher: CipherSuite,
}

/// Result of a completed client handshake.
pub struct ClientHandshakeResult {
    pub session_keys: SessionKeys,
    pub client_hello_bytes: Vec<u8>,
}

/// Intermediate state after ClientHello is sent.
pub struct ClientAwaitingServerHello {
    keypair: EphemeralKeyPair,
    client_pub: [u8; 32],
    timestamp: u64,
    client_id: ClientId,
    auth_secret: [u8; 32],
    preferred_cipher: CipherSuite,
}

impl ClientHandshake {
    pub fn new(client_id: ClientId, auth_secret: [u8; 32], preferred_cipher: CipherSuite) -> Self {
        Self {
            client_id,
            auth_secret,
            preferred_cipher,
        }
    }

    /// Step 1: Generate ClientHello and transition to awaiting ServerHello.
    pub fn start(self) -> (ClientAwaitingServerHello, Vec<u8>) {
        let keypair = EphemeralKeyPair::generate();
        let client_pub = keypair.public_key_bytes();
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let hello = ClientHello {
            version: PROTOCOL_VERSION,
            client_ephemeral_pub: client_pub,
            timestamp,
            padding: crate::crypto::padding::generate_padding(64),
        };
        let bytes = encode_client_hello(&hello);

        let state = ClientAwaitingServerHello {
            keypair,
            client_pub,
            timestamp,
            client_id: self.client_id,
            auth_secret: self.auth_secret,
            preferred_cipher: self.preferred_cipher,
        };

        (state, bytes)
    }
}

impl ClientAwaitingServerHello {
    /// Step 3: Process ServerHello and produce ClientAuth + await ServerAccept.
    pub fn process_server_hello(
        self,
        server_hello_bytes: &[u8],
    ) -> Result<(Vec<u8>, ClientAwaitingAccept), PrismaError> {
        let server_hello = decode_server_hello(server_hello_bytes)?;

        // Derive shared secret
        let server_pub_key = x25519_dalek::PublicKey::from(server_hello.server_ephemeral_pub);
        let shared_secret = self.keypair.diffie_hellman(&server_pub_key);

        // Derive session key
        let session_key = derive_session_key(
            &shared_secret,
            &self.client_pub,
            &server_hello.server_ephemeral_pub,
            self.timestamp,
        );

        // Decrypt challenge
        let cipher = create_cipher(CipherSuite::ChaCha20Poly1305, &session_key);
        let challenge_nonce = [0u8; NONCE_SIZE];
        let challenge = cipher
            .decrypt(&challenge_nonce, &server_hello.encrypted_challenge, &[])
            .map_err(|e| ProtocolError::HandshakeFailed(format!("Challenge decrypt: {}", e)))?;

        // Compute challenge response: BLAKE3 hash of challenge
        let challenge_response: [u8; 32] = blake3::hash(&challenge).into();

        // Compute auth token: HMAC-SHA256(auth_secret, client_id || timestamp)
        let auth_token = util::compute_auth_token(&self.auth_secret, &self.client_id, self.timestamp);

        // Build ClientAuth
        let client_auth = ClientAuth {
            client_id: self.client_id,
            auth_token,
            cipher_suite: self.preferred_cipher,
            challenge_response,
        };

        // Encrypt ClientAuth
        let auth_plaintext = encode_client_auth(&client_auth);
        let mut auth_nonce_bytes = [0u8; NONCE_SIZE];
        auth_nonce_bytes[11] = 1; // Use nonce counter = 1
        let auth_encrypted = cipher
            .encrypt(&auth_nonce_bytes, &auth_plaintext, &[])
            .map_err(|e| {
                PrismaError::Crypto(CryptoError::EncryptionFailed(format!(
                    "ClientAuth encrypt: {}",
                    e
                )))
            })?;

        let state = ClientAwaitingAccept {
            session_key,
            cipher_suite: self.preferred_cipher,
            client_id: self.client_id,
        };

        Ok((auth_encrypted, state))
    }
}

pub struct ClientAwaitingAccept {
    session_key: [u8; 32],
    cipher_suite: CipherSuite,
    client_id: ClientId,
}

impl ClientAwaitingAccept {
    /// Step 5 (client side): Process ServerAccept.
    pub fn process_server_accept(self, accept_bytes: &[u8]) -> Result<SessionKeys, PrismaError> {
        // ServerAccept is encrypted with the session key
        let cipher = create_cipher(CipherSuite::ChaCha20Poly1305, &self.session_key);
        let mut nonce = [0u8; NONCE_SIZE];
        nonce[11] = 2; // nonce counter = 2

        let plaintext = cipher
            .decrypt(&nonce, accept_bytes, &[])
            .map_err(|e| ProtocolError::HandshakeFailed(format!("Accept decrypt: {}", e)))?;

        let accept = decode_server_accept(&plaintext)?;

        if accept.status != AcceptStatus::Ok {
            return Err(PrismaError::Auth(format!(
                "Server rejected: {:?}",
                accept.status
            )));
        }

        Ok(SessionKeys {
            session_key: self.session_key,
            cipher_suite: self.cipher_suite,
            session_id: accept.session_id,
            client_id: self.client_id,
            client_nonce_counter: 0,
            server_nonce_counter: 0,
        })
    }
}

/// Server-side handshake state machine.
pub struct ServerHandshake;

pub struct ServerAwaitingClientAuth {
    session_key: [u8; 32],
    challenge: Vec<u8>,
    #[allow(dead_code)]
    server_pub: [u8; 32],
    timestamp: u64,
}

/// Callback for the server to verify client credentials.
pub trait AuthVerifier: Send + Sync {
    fn verify(&self, client_id: &ClientId, auth_token: &[u8; 32], timestamp: u64) -> bool;
}

impl ServerHandshake {
    /// Step 2: Process ClientHello, generate ServerHello.
    pub fn process_client_hello(
        client_hello_bytes: &[u8],
    ) -> Result<(Vec<u8>, ServerAwaitingClientAuth), PrismaError> {
        let client_hello = decode_client_hello(client_hello_bytes)?;

        if client_hello.version != PROTOCOL_VERSION {
            return Err(ProtocolError::InvalidVersion(client_hello.version).into());
        }

        // Generate server ephemeral key
        let server_keypair = EphemeralKeyPair::generate();
        let server_pub = server_keypair.public_key_bytes();

        // Derive shared secret
        let client_pub_key = x25519_dalek::PublicKey::from(client_hello.client_ephemeral_pub);
        let shared_secret = server_keypair.diffie_hellman(&client_pub_key);

        // Derive session key
        let session_key = derive_session_key(
            &shared_secret,
            &client_hello.client_ephemeral_pub,
            &server_pub,
            client_hello.timestamp,
        );

        // Generate challenge
        let mut challenge = vec![0u8; 32];
        rand::Rng::fill(&mut rand::thread_rng(), &mut challenge[..]);

        // Encrypt challenge
        let cipher = create_cipher(CipherSuite::ChaCha20Poly1305, &session_key);
        let challenge_nonce = [0u8; NONCE_SIZE];
        let encrypted_challenge =
            cipher
                .encrypt(&challenge_nonce, &challenge, &[])
                .map_err(|e| {
                    PrismaError::Crypto(CryptoError::EncryptionFailed(format!(
                        "Challenge encrypt: {}",
                        e
                    )))
                })?;

        let server_hello = ServerHello {
            server_ephemeral_pub: server_pub,
            encrypted_challenge,
            padding: crate::crypto::padding::generate_padding(64),
        };
        let bytes = encode_server_hello(&server_hello);

        let state = ServerAwaitingClientAuth {
            session_key,
            challenge,
            server_pub,
            timestamp: client_hello.timestamp,
        };

        Ok((bytes, state))
    }
}

impl ServerAwaitingClientAuth {
    /// Step 4: Process ClientAuth, verify credentials, produce ServerAccept.
    pub fn process_client_auth(
        self,
        encrypted_auth: &[u8],
        verifier: &dyn AuthVerifier,
    ) -> Result<(Vec<u8>, SessionKeys), PrismaError> {
        // Decrypt ClientAuth
        let cipher = create_cipher(CipherSuite::ChaCha20Poly1305, &self.session_key);
        let mut nonce = [0u8; NONCE_SIZE];
        nonce[11] = 1; // Same nonce counter as client used

        let auth_plaintext = cipher
            .decrypt(&nonce, encrypted_auth, &[])
            .map_err(|e| ProtocolError::HandshakeFailed(format!("Auth decrypt: {}", e)))?;

        let client_auth = decode_client_auth(&auth_plaintext)?;

        // Verify challenge response
        let expected_response: [u8; 32] = blake3::hash(&self.challenge).into();
        if client_auth.challenge_response != expected_response {
            return Err(PrismaError::Auth("Invalid challenge response".into()));
        }

        // Verify client credentials
        if !verifier.verify(
            &client_auth.client_id,
            &client_auth.auth_token,
            self.timestamp,
        ) {
            return Err(PrismaError::Auth("Authentication failed".into()));
        }

        // Build ServerAccept
        let session_id = Uuid::new_v4();
        let accept = ServerAccept {
            status: AcceptStatus::Ok,
            session_id,
        };
        let accept_plaintext = encode_server_accept(&accept);
        let mut accept_nonce = [0u8; NONCE_SIZE];
        accept_nonce[11] = 2;
        let encrypted_accept = cipher
            .encrypt(&accept_nonce, &accept_plaintext, &[])
            .map_err(|e| {
                PrismaError::Crypto(CryptoError::EncryptionFailed(format!(
                    "Accept encrypt: {}",
                    e
                )))
            })?;

        let session_keys = SessionKeys {
            session_key: self.session_key,
            cipher_suite: client_auth.cipher_suite,
            session_id,
            client_id: client_auth.client_id,
            client_nonce_counter: 0,
            server_nonce_counter: 0,
        };

        Ok((encrypted_accept, session_keys))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestVerifier {
        expected_id: ClientId,
        auth_secret: [u8; 32],
    }

    impl AuthVerifier for TestVerifier {
        fn verify(&self, client_id: &ClientId, auth_token: &[u8; 32], timestamp: u64) -> bool {
            if *client_id != self.expected_id {
                return false;
            }
            let expected = util::compute_auth_token(&self.auth_secret, client_id, timestamp);
            util::ct_eq(auth_token, &expected)
        }
    }

    #[test]
    fn test_full_handshake() {
        let client_id = ClientId::new();
        let auth_secret = [0x42u8; 32];

        // Client step 1
        let client_hs = ClientHandshake::new(client_id, auth_secret, CipherSuite::ChaCha20Poly1305);
        let (client_state, client_hello_bytes) = client_hs.start();

        // Server step 2
        let (server_hello_bytes, server_state) =
            ServerHandshake::process_client_hello(&client_hello_bytes).unwrap();

        // Client step 3
        let (client_auth_bytes, client_accept_state) = client_state
            .process_server_hello(&server_hello_bytes)
            .unwrap();

        // Server step 4
        let verifier = TestVerifier {
            expected_id: client_id,
            auth_secret,
        };
        let (server_accept_bytes, server_session) = server_state
            .process_client_auth(&client_auth_bytes, &verifier)
            .unwrap();

        // Client step 5
        let client_session = client_accept_state
            .process_server_accept(&server_accept_bytes)
            .unwrap();

        // Both sides should have the same session key
        assert_eq!(client_session.session_key, server_session.session_key);
        assert_eq!(client_session.session_id, server_session.session_id);
        assert_eq!(client_session.cipher_suite, server_session.cipher_suite);
    }

    #[test]
    fn test_handshake_bad_auth() {
        let client_id = ClientId::new();
        let client_secret = [0x42u8; 32];
        let wrong_secret = [0x99u8; 32]; // Different secret on server

        let client_hs =
            ClientHandshake::new(client_id, client_secret, CipherSuite::ChaCha20Poly1305);
        let (client_state, client_hello_bytes) = client_hs.start();

        let (server_hello_bytes, server_state) =
            ServerHandshake::process_client_hello(&client_hello_bytes).unwrap();

        let (client_auth_bytes, _client_accept_state) = client_state
            .process_server_hello(&server_hello_bytes)
            .unwrap();

        let verifier = TestVerifier {
            expected_id: client_id,
            auth_secret: wrong_secret,
        };

        let result = server_state.process_client_auth(&client_auth_bytes, &verifier);
        assert!(result.is_err());
    }
}
