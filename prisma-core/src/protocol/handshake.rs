use uuid::Uuid;

use crate::crypto::aead::create_cipher;
use crate::crypto::ecdh::EphemeralKeyPair;
use crate::crypto::kdf::{derive_preliminary_key, derive_v3_session_key};
use crate::error::{CryptoError, PrismaError, ProtocolError};
use crate::protocol::codec::*;
use crate::protocol::types::*;
use crate::types::{CipherSuite, ClientId, PaddingRange, NONCE_SIZE, PRISMA_PROTOCOL_VERSION};
use crate::util;

// ===== Prisma Handshake (2-step: PrismaClientInit → PrismaServerInit) =====

/// Callback for the server to verify client credentials.
pub trait AuthVerifier: Send + Sync {
    fn verify(&self, client_id: &ClientId, auth_token: &[u8; 32], timestamp: u64) -> bool;
}

/// Client-side handshake state machine.
pub struct PrismaHandshakeClient {
    client_id: ClientId,
    auth_secret: [u8; 32],
    preferred_cipher: CipherSuite,
}

impl PrismaHandshakeClient {
    pub fn new(client_id: ClientId, auth_secret: [u8; 32], preferred_cipher: CipherSuite) -> Self {
        Self {
            client_id,
            auth_secret,
            preferred_cipher,
        }
    }

    /// Step 1: Generate PrismaClientInit and transition to awaiting PrismaServerInit.
    pub fn start(self) -> (PrismaClientAwaitingServerInit, Vec<u8>) {
        let keypair = EphemeralKeyPair::generate();
        let client_pub = keypair.public_key_bytes();
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let auth_token = util::compute_auth_token(&self.auth_secret, &self.client_id, timestamp);

        let init = PrismaClientInit {
            version: PRISMA_PROTOCOL_VERSION,
            flags: 0,
            client_ephemeral_pub: client_pub,
            client_id: self.client_id,
            timestamp,
            cipher_suite: self.preferred_cipher,
            auth_token,
            padding: crate::crypto::padding::generate_padding(64),
        };
        let bytes = encode_client_init(&init);

        let state = PrismaClientAwaitingServerInit {
            keypair,
            client_pub,
            timestamp,
            client_id: self.client_id,
            preferred_cipher: self.preferred_cipher,
        };

        (state, bytes)
    }
}

/// Intermediate state after PrismaClientInit is sent.
pub struct PrismaClientAwaitingServerInit {
    keypair: EphemeralKeyPair,
    client_pub: [u8; 32],
    timestamp: u64,
    client_id: ClientId,
    preferred_cipher: CipherSuite,
}

impl PrismaClientAwaitingServerInit {
    /// Process PrismaServerInit and produce session keys.
    /// Returns (SessionKeys, bucket_sizes).
    /// The client must then send a ChallengeResponse as its first data frame.
    pub fn process_server_init(
        self,
        encrypted_server_init: &[u8],
    ) -> Result<(SessionKeys, Vec<u16>), PrismaError> {
        if encrypted_server_init.len() < 32 + NONCE_SIZE + 2 {
            return Err(ProtocolError::InvalidFrame("PrismaServerInit too short".into()).into());
        }

        // Extract server's public key from clear prefix
        let mut server_pub = [0u8; 32];
        server_pub.copy_from_slice(&encrypted_server_init[..32]);
        let encrypted_payload = &encrypted_server_init[32..];

        // Derive shared secret
        let server_pub_key = x25519_dalek::PublicKey::from(server_pub);
        let shared_secret = self.keypair.diffie_hellman(&server_pub_key);

        // Derive preliminary key
        let prelim_key = derive_preliminary_key(
            &shared_secret,
            &self.client_pub,
            &server_pub,
            self.timestamp,
        );

        // Decrypt PrismaServerInit
        let cipher = create_cipher(CipherSuite::ChaCha20Poly1305, &prelim_key);
        let (plaintext, _nonce) =
            decrypt_frame(cipher.as_ref(), encrypted_payload).map_err(|e| {
                ProtocolError::HandshakeFailed(format!("PrismaServerInit decrypt: {}", e))
            })?;

        let server_init = decode_server_init(&plaintext)?;

        if server_init.status != AcceptStatus::Ok {
            return Err(PrismaError::Auth(format!(
                "Server rejected: {:?}",
                server_init.status
            )));
        }

        // Derive final session key
        let session_key = derive_v3_session_key(
            &shared_secret,
            &self.client_pub,
            &server_init.server_ephemeral_pub,
            &server_init.challenge,
            self.timestamp,
        );

        let padding_range = PaddingRange::new(server_init.padding_min, server_init.padding_max);
        let bucket_sizes = server_init.bucket_sizes.clone();

        Ok((
            SessionKeys {
                session_key,
                cipher_suite: self.preferred_cipher,
                session_id: server_init.session_id,
                client_id: self.client_id,
                client_nonce_counter: 0,
                server_nonce_counter: 0,
                protocol_version: PRISMA_PROTOCOL_VERSION,
                padding_range,
                challenge: Some(server_init.challenge),
                session_ticket: if server_init.session_ticket.is_empty() {
                    None
                } else {
                    Some(server_init.session_ticket)
                },
            },
            bucket_sizes,
        ))
    }
}

/// Server-side handshake.
pub struct PrismaHandshakeServer;

impl PrismaHandshakeServer {
    /// Process a PrismaClientInit and produce an encrypted PrismaServerInit response.
    ///
    /// Returns: (encrypted_server_init_bytes, PrismaServerCompleted)
    pub fn process_client_init(
        client_init_bytes: &[u8],
        padding_range: PaddingRange,
        server_features: u32,
        ticket_key: &[u8; 32],
        bucket_sizes: &[u16],
        verifier: &dyn AuthVerifier,
    ) -> Result<(Vec<u8>, PrismaServerCompleted), PrismaError> {
        let client_init = decode_client_init(client_init_bytes)?;

        if client_init.version != PRISMA_PROTOCOL_VERSION {
            return Err(ProtocolError::InvalidVersion(client_init.version).into());
        }

        // Verify auth token
        if !verifier.verify(
            &client_init.client_id,
            &client_init.auth_token,
            client_init.timestamp,
        ) {
            return Err(PrismaError::Auth("Authentication failed".into()));
        }

        // Generate server ephemeral key
        let server_keypair = EphemeralKeyPair::generate();
        let server_pub = server_keypair.public_key_bytes();

        // Derive shared secret
        let client_pub_key = x25519_dalek::PublicKey::from(client_init.client_ephemeral_pub);
        let shared_secret = server_keypair.diffie_hellman(&client_pub_key);

        // Generate random challenge
        let mut challenge = [0u8; 32];
        rand::Rng::fill(&mut rand::thread_rng(), &mut challenge[..]);

        let session_id = Uuid::new_v4();

        // Derive preliminary key (for encrypting PrismaServerInit)
        let prelim_key = derive_preliminary_key(
            &shared_secret,
            &client_init.client_ephemeral_pub,
            &server_pub,
            client_init.timestamp,
        );

        // Derive final session key (for data transfer)
        let session_key = derive_v3_session_key(
            &shared_secret,
            &client_init.client_ephemeral_pub,
            &server_pub,
            &challenge,
            client_init.timestamp,
        );

        // Create session ticket
        let ticket_plaintext = encode_session_ticket(&SessionTicket {
            client_id: client_init.client_id,
            session_key,
            cipher_suite: client_init.cipher_suite,
            issued_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            padding_range,
        });
        let ticket_cipher = create_cipher(CipherSuite::ChaCha20Poly1305, ticket_key);
        let ticket_nonce = [0u8; NONCE_SIZE];
        let encrypted_ticket = ticket_cipher
            .encrypt(&ticket_nonce, &ticket_plaintext, &[])
            .map_err(|e| CryptoError::EncryptionFailed(format!("Ticket encrypt: {}", e)))?;

        // Build PrismaServerInit
        let server_init = PrismaServerInit {
            status: AcceptStatus::Ok,
            session_id,
            server_ephemeral_pub: server_pub,
            challenge,
            padding_min: padding_range.min,
            padding_max: padding_range.max,
            server_features,
            session_ticket: encrypted_ticket,
            bucket_sizes: bucket_sizes.to_vec(),
            padding: crate::crypto::padding::generate_padding(64),
        };
        let init_plaintext = encode_server_init(&server_init);

        // Encrypt with preliminary key
        let prelim_cipher = create_cipher(CipherSuite::ChaCha20Poly1305, &prelim_key);
        let init_nonce = [0u8; NONCE_SIZE];
        let encrypted_init = encrypt_frame(prelim_cipher.as_ref(), &init_nonce, &init_plaintext)
            .map_err(|e| {
                CryptoError::EncryptionFailed(format!("PrismaServerInit encrypt: {}", e))
            })?;

        // Wire: [server_pub:32][encrypted_frame]
        let mut wire = Vec::with_capacity(32 + encrypted_init.len());
        wire.extend_from_slice(&server_pub);
        wire.extend_from_slice(&encrypted_init);

        let state = PrismaServerCompleted {
            session_key,
            challenge,
            session_id,
            client_id: client_init.client_id,
            cipher_suite: client_init.cipher_suite,
            padding_range,
        };

        Ok((wire, state))
    }
}

/// Server state after handshake: waiting for client's challenge response in first data frame.
pub struct PrismaServerCompleted {
    pub session_key: [u8; 32],
    pub challenge: [u8; 32],
    pub session_id: Uuid,
    pub client_id: ClientId,
    pub cipher_suite: CipherSuite,
    pub padding_range: PaddingRange,
}

impl PrismaServerCompleted {
    /// Complete the handshake by producing SessionKeys.
    /// The server should verify the challenge response from the first data frame separately.
    pub fn into_session_keys(self) -> SessionKeys {
        SessionKeys {
            session_key: self.session_key,
            cipher_suite: self.cipher_suite,
            session_id: self.session_id,
            client_id: self.client_id,
            client_nonce_counter: 0,
            server_nonce_counter: 0,
            protocol_version: PRISMA_PROTOCOL_VERSION,
            padding_range: self.padding_range,
            challenge: Some(self.challenge),
            session_ticket: None,
        }
    }

    /// Verify a challenge response hash.
    pub fn verify_challenge(&self, response_hash: &[u8; 32]) -> bool {
        let expected: [u8; 32] = blake3::hash(&self.challenge).into();
        util::ct_eq(response_hash, &expected)
    }
}

// ===== Version detection helper =====

/// Check if a version byte is the supported Prisma protocol version.
pub fn is_valid_protocol_version(version: u8) -> bool {
    version == PRISMA_PROTOCOL_VERSION
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

    fn server_state_verify_challenge(challenge: &[u8; 32], response_hash: &[u8; 32]) -> bool {
        let expected: [u8; 32] = blake3::hash(challenge).into();
        util::ct_eq(response_hash, &expected)
    }

    #[test]
    fn test_prisma_handshake() {
        let client_id = ClientId::new();
        let auth_secret = [0x42u8; 32];
        let ticket_key = [0xFFu8; 32];
        let bucket_sizes = vec![128, 256, 512, 1024, 2048, 4096, 8192, 16384];

        let verifier = TestVerifier {
            expected_id: client_id,
            auth_secret,
        };

        // Client step 1: send PrismaClientInit
        let client_hs =
            PrismaHandshakeClient::new(client_id, auth_secret, CipherSuite::ChaCha20Poly1305);
        let (client_state, client_init_bytes) = client_hs.start();

        // Server step 1: process PrismaClientInit, produce PrismaServerInit
        let padding_range = PaddingRange::new(0, 256);
        let (server_init_bytes, server_state) = PrismaHandshakeServer::process_client_init(
            &client_init_bytes,
            padding_range,
            FEATURE_UDP_RELAY | FEATURE_SPEED_TEST,
            &ticket_key,
            &bucket_sizes,
            &verifier,
        )
        .unwrap();

        // Client step 2: process PrismaServerInit
        let (client_session, client_buckets) = client_state
            .process_server_init(&server_init_bytes)
            .unwrap();

        // Server produces session keys
        let server_session = server_state.into_session_keys();

        // Both sides should have the same session key
        assert_eq!(client_session.session_key, server_session.session_key);
        assert_eq!(client_session.session_id, server_session.session_id);
        assert_eq!(client_session.cipher_suite, server_session.cipher_suite);
        assert_eq!(client_session.protocol_version, PRISMA_PROTOCOL_VERSION);

        // Client should have a challenge to respond to
        assert!(client_session.challenge.is_some());
        assert!(client_session.session_ticket.is_some());

        // Client should receive bucket sizes
        assert_eq!(client_buckets, bucket_sizes);

        // Verify challenge response
        let challenge = client_session.challenge.unwrap();
        let response_hash: [u8; 32] = blake3::hash(&challenge).into();
        assert!(server_state_verify_challenge(
            &server_session.challenge.unwrap(),
            &response_hash
        ));
    }

    #[test]
    fn test_prisma_handshake_bad_auth() {
        let client_id = ClientId::new();
        let client_secret = [0x42u8; 32];
        let wrong_secret = [0x99u8; 32];
        let ticket_key = [0xFFu8; 32];

        let verifier = TestVerifier {
            expected_id: client_id,
            auth_secret: wrong_secret,
        };

        let client_hs =
            PrismaHandshakeClient::new(client_id, client_secret, CipherSuite::ChaCha20Poly1305);
        let (_, client_init_bytes) = client_hs.start();

        let result = PrismaHandshakeServer::process_client_init(
            &client_init_bytes,
            PaddingRange::new(0, 256),
            0,
            &ticket_key,
            &[128, 256, 512],
            &verifier,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_prisma_handshake_wrong_client_id() {
        let client_id = ClientId::new();
        let other_id = ClientId::new();
        let auth_secret = [0x42u8; 32];
        let ticket_key = [0xFFu8; 32];

        let verifier = TestVerifier {
            expected_id: other_id, // Verifier expects a different client
            auth_secret,
        };

        let client_hs =
            PrismaHandshakeClient::new(client_id, auth_secret, CipherSuite::ChaCha20Poly1305);
        let (_, client_init_bytes) = client_hs.start();

        let result = PrismaHandshakeServer::process_client_init(
            &client_init_bytes,
            PaddingRange::new(0, 256),
            0,
            &ticket_key,
            &[],
            &verifier,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_prisma_handshake_aes256gcm() {
        let client_id = ClientId::new();
        let auth_secret = [0x42u8; 32];
        let ticket_key = [0xFFu8; 32];

        let verifier = TestVerifier {
            expected_id: client_id,
            auth_secret,
        };

        // Use AES-256-GCM cipher suite
        let client_hs = PrismaHandshakeClient::new(client_id, auth_secret, CipherSuite::Aes256Gcm);
        let (client_state, client_init_bytes) = client_hs.start();

        let padding_range = PaddingRange::new(10, 128);
        let (server_init_bytes, server_state) = PrismaHandshakeServer::process_client_init(
            &client_init_bytes,
            padding_range,
            0,
            &ticket_key,
            &[],
            &verifier,
        )
        .unwrap();

        let (client_session, client_buckets) = client_state
            .process_server_init(&server_init_bytes)
            .unwrap();

        let server_session = server_state.into_session_keys();

        assert_eq!(client_session.session_key, server_session.session_key);
        assert_eq!(client_session.cipher_suite, CipherSuite::Aes256Gcm);
        assert_eq!(server_session.cipher_suite, CipherSuite::Aes256Gcm);
        assert_eq!(client_session.padding_range, padding_range);
        assert!(client_buckets.is_empty());
    }

    #[test]
    fn test_prisma_handshake_no_buckets() {
        let client_id = ClientId::new();
        let auth_secret = [0x42u8; 32];
        let ticket_key = [0xFFu8; 32];

        let verifier = TestVerifier {
            expected_id: client_id,
            auth_secret,
        };

        let client_hs =
            PrismaHandshakeClient::new(client_id, auth_secret, CipherSuite::ChaCha20Poly1305);
        let (client_state, client_init_bytes) = client_hs.start();

        let (server_init_bytes, server_state) = PrismaHandshakeServer::process_client_init(
            &client_init_bytes,
            PaddingRange::new(0, 256),
            FEATURE_UDP_RELAY | FEATURE_FEC | FEATURE_DNS_TUNNEL,
            &ticket_key,
            &[], // no bucket sizes
            &verifier,
        )
        .unwrap();

        let (client_session, client_buckets) = client_state
            .process_server_init(&server_init_bytes)
            .unwrap();
        let server_session = server_state.into_session_keys();

        assert_eq!(client_session.session_key, server_session.session_key);
        assert!(client_buckets.is_empty());
    }

    #[test]
    fn test_prisma_challenge_verification() {
        let client_id = ClientId::new();
        let auth_secret = [0x42u8; 32];
        let ticket_key = [0xFFu8; 32];

        let verifier = TestVerifier {
            expected_id: client_id,
            auth_secret,
        };

        let client_hs =
            PrismaHandshakeClient::new(client_id, auth_secret, CipherSuite::ChaCha20Poly1305);
        let (client_state, client_init_bytes) = client_hs.start();

        let (server_init_bytes, server_state) = PrismaHandshakeServer::process_client_init(
            &client_init_bytes,
            PaddingRange::new(0, 256),
            0,
            &ticket_key,
            &[],
            &verifier,
        )
        .unwrap();

        let (client_session, _) = client_state
            .process_server_init(&server_init_bytes)
            .unwrap();

        // Correct challenge response should verify
        let challenge = client_session.challenge.unwrap();
        let correct_hash: [u8; 32] = blake3::hash(&challenge).into();
        assert!(server_state.verify_challenge(&correct_hash));

        // Wrong challenge response should fail
        let wrong_hash = [0xFFu8; 32];
        assert!(!server_state.verify_challenge(&wrong_hash));
    }

    #[test]
    fn test_prisma_handshake_session_ticket_present() {
        let client_id = ClientId::new();
        let auth_secret = [0x42u8; 32];
        let ticket_key = [0xFFu8; 32];

        let verifier = TestVerifier {
            expected_id: client_id,
            auth_secret,
        };

        let client_hs =
            PrismaHandshakeClient::new(client_id, auth_secret, CipherSuite::ChaCha20Poly1305);
        let (client_state, client_init_bytes) = client_hs.start();

        let (server_init_bytes, _server_state) = PrismaHandshakeServer::process_client_init(
            &client_init_bytes,
            PaddingRange::new(0, 256),
            0,
            &ticket_key,
            &[],
            &verifier,
        )
        .unwrap();

        let (client_session, _) = client_state
            .process_server_init(&server_init_bytes)
            .unwrap();

        // Session ticket should be present (server always generates one)
        assert!(client_session.session_ticket.is_some());
        let ticket_bytes = client_session.session_ticket.unwrap();
        assert!(!ticket_bytes.is_empty());
    }

    #[test]
    fn test_prisma_version_detection() {
        assert!(is_valid_protocol_version(PRISMA_PROTOCOL_VERSION));
        assert!(is_valid_protocol_version(0x04));
        // Old versions should be invalid
        assert!(!is_valid_protocol_version(0x01));
        assert!(!is_valid_protocol_version(0x02));
        assert!(!is_valid_protocol_version(0x03));
        // Unknown versions should be invalid
        assert!(!is_valid_protocol_version(0x05));
        assert!(!is_valid_protocol_version(0x00));
    }

    #[test]
    fn test_prisma_server_init_too_short() {
        let client_id = ClientId::new();
        let auth_secret = [0x42u8; 32];

        let client_hs =
            PrismaHandshakeClient::new(client_id, auth_secret, CipherSuite::ChaCha20Poly1305);
        let (client_state, _) = client_hs.start();

        // Too-short data should fail
        let short_data = vec![0u8; 10];
        let result = client_state.process_server_init(&short_data);
        assert!(result.is_err());
    }

    #[test]
    fn test_prisma_client_init_version_mismatch() {
        let client_id = ClientId::new();
        let auth_secret = [0x42u8; 32];
        let ticket_key = [0xFFu8; 32];

        let verifier = TestVerifier {
            expected_id: client_id,
            auth_secret,
        };

        // Manually construct a PrismaClientInit with wrong version
        let keypair = EphemeralKeyPair::generate();
        let client_pub = keypair.public_key_bytes();
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let auth_token = util::compute_auth_token(&auth_secret, &client_id, timestamp);

        let bad_init = PrismaClientInit {
            version: 0x03, // Wrong version
            flags: 0,
            client_ephemeral_pub: client_pub,
            client_id,
            timestamp,
            cipher_suite: CipherSuite::ChaCha20Poly1305,
            auth_token,
            padding: vec![],
        };
        let bad_bytes = encode_client_init(&bad_init);

        let result = PrismaHandshakeServer::process_client_init(
            &bad_bytes,
            PaddingRange::new(0, 256),
            0,
            &ticket_key,
            &[],
            &verifier,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_prisma_server_completed_into_session_keys() {
        let session_key = [0xAAu8; 32];
        let challenge = [0xBBu8; 32];
        let session_id = Uuid::new_v4();
        let client_id = ClientId::new();
        let padding_range = PaddingRange::new(10, 200);

        let completed = PrismaServerCompleted {
            session_key,
            challenge,
            session_id,
            client_id,
            cipher_suite: CipherSuite::ChaCha20Poly1305,
            padding_range,
        };

        let keys = completed.into_session_keys();
        assert_eq!(keys.session_key, session_key);
        assert_eq!(keys.session_id, session_id);
        assert_eq!(keys.client_id, client_id);
        assert_eq!(keys.cipher_suite, CipherSuite::ChaCha20Poly1305);
        assert_eq!(keys.protocol_version, PRISMA_PROTOCOL_VERSION);
        assert_eq!(keys.padding_range, padding_range);
        assert_eq!(keys.challenge, Some(challenge));
        assert_eq!(keys.session_ticket, None);
        assert_eq!(keys.client_nonce_counter, 0);
        assert_eq!(keys.server_nonce_counter, 0);
    }
}
