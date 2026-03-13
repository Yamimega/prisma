use uuid::Uuid;

use crate::crypto::aead::create_cipher;
use crate::crypto::ecdh::EphemeralKeyPair;
use crate::crypto::kdf::{derive_preliminary_key, derive_session_key, derive_v3_session_key};
use crate::error::{CryptoError, PrismaError, ProtocolError};
use crate::protocol::codec::*;
use crate::protocol::types::*;
use crate::types::{
    CipherSuite, ClientId, PaddingRange, DEFAULT_PADDING_RANGE, NONCE_SIZE, PROTOCOL_VERSION,
    PROTOCOL_VERSION_V1, PROTOCOL_VERSION_V2,
};
use crate::util;

// ===== v3 Handshake (2-step: ClientInit → ServerInit) =====

/// v3 Client-side handshake state machine.
pub struct ClientHandshakeV3 {
    client_id: ClientId,
    auth_secret: [u8; 32],
    preferred_cipher: CipherSuite,
}

impl ClientHandshakeV3 {
    pub fn new(client_id: ClientId, auth_secret: [u8; 32], preferred_cipher: CipherSuite) -> Self {
        Self {
            client_id,
            auth_secret,
            preferred_cipher,
        }
    }

    /// Step 1: Generate ClientInit and transition to awaiting ServerInit.
    pub fn start(self) -> (ClientAwaitingServerInit, Vec<u8>) {
        let keypair = EphemeralKeyPair::generate();
        let client_pub = keypair.public_key_bytes();
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let auth_token = util::compute_auth_token(&self.auth_secret, &self.client_id, timestamp);

        let init = ClientInit {
            version: PROTOCOL_VERSION,
            flags: 0,
            client_ephemeral_pub: client_pub,
            client_id: self.client_id,
            timestamp,
            cipher_suite: self.preferred_cipher,
            auth_token,
            padding: crate::crypto::padding::generate_padding(64),
        };
        let bytes = encode_client_init(&init);

        let state = ClientAwaitingServerInit {
            keypair,
            client_pub,
            timestamp,
            client_id: self.client_id,
            preferred_cipher: self.preferred_cipher,
        };

        (state, bytes)
    }
}

/// Intermediate state after ClientInit is sent (v3).
pub struct ClientAwaitingServerInit {
    keypair: EphemeralKeyPair,
    client_pub: [u8; 32],
    timestamp: u64,
    client_id: ClientId,
    preferred_cipher: CipherSuite,
}

impl ClientAwaitingServerInit {
    /// Process ServerInit and produce session keys.
    /// The client must then send a ChallengeResponse as its first data frame.
    pub fn process_server_init(
        self,
        encrypted_server_init: &[u8],
    ) -> Result<SessionKeys, PrismaError> {
        // To decrypt ServerInit, we need the preliminary key.
        // But we don't have server_pub yet — it's inside the encrypted message.
        // The preliminary key is derived from the DH shared secret using client_pub
        // and the server's ephemeral pub (which is the first 32 bytes after the
        // nonce+len prefix in the encrypted frame).
        //
        // Actually, the encrypted ServerInit is sent as:
        //   [nonce:12][len:2][ciphertext:var][tag:16]
        // We need to extract the nonce, but the key for decryption requires server_pub
        // which is embedded in the plaintext.
        //
        // Solution: The server encrypts with a key derived from:
        //   BLAKE3("prisma-v3-preliminary", shared_secret || client_pub || server_pub || timestamp)
        // The server_pub is sent in the CLEAR as a prefix before the encrypted portion:
        //   Wire: [server_ephemeral_pub:32][nonce:12][len:2][ciphertext:var][tag:16]

        if encrypted_server_init.len() < 32 + NONCE_SIZE + 2 {
            return Err(ProtocolError::InvalidFrame("ServerInit too short".into()).into());
        }

        // Extract server's public key from the clear prefix
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

        // Decrypt ServerInit
        let cipher = create_cipher(CipherSuite::ChaCha20Poly1305, &prelim_key);
        let (plaintext, _nonce) = decrypt_frame(cipher.as_ref(), encrypted_payload)
            .map_err(|e| ProtocolError::HandshakeFailed(format!("ServerInit decrypt: {}", e)))?;

        let server_init = decode_server_init(&plaintext)?;

        if server_init.status != AcceptStatus::Ok {
            return Err(PrismaError::Auth(format!(
                "Server rejected: {:?}",
                server_init.status
            )));
        }

        // Derive final session key (Phase 2: with challenge binding)
        let session_key = derive_v3_session_key(
            &shared_secret,
            &self.client_pub,
            &server_init.server_ephemeral_pub,
            &server_init.challenge,
            self.timestamp,
        );

        let padding_range = PaddingRange::new(server_init.padding_min, server_init.padding_max);

        Ok(SessionKeys {
            session_key,
            cipher_suite: self.preferred_cipher,
            session_id: server_init.session_id,
            client_id: self.client_id,
            client_nonce_counter: 0,
            server_nonce_counter: 0,
            protocol_version: PROTOCOL_VERSION,
            padding_range,
            challenge: Some(server_init.challenge),
            session_ticket: if server_init.session_ticket.is_empty() {
                None
            } else {
                Some(server_init.session_ticket)
            },
        })
    }
}

/// v3 Server-side handshake.
pub struct ServerHandshakeV3;

impl ServerHandshakeV3 {
    /// Process a v3 ClientInit and produce an encrypted ServerInit response.
    ///
    /// Returns: (encrypted_server_init_bytes, ServerAwaitingChallengeV3)
    pub fn process_client_init(
        client_init_bytes: &[u8],
        padding_range: PaddingRange,
        server_features: u32,
        ticket_key: &[u8; 32],
        verifier: &dyn AuthVerifier,
    ) -> Result<(Vec<u8>, ServerAwaitingChallengeV3), PrismaError> {
        let client_init = decode_client_init(client_init_bytes)?;

        if client_init.version != PROTOCOL_VERSION {
            return Err(ProtocolError::InvalidVersion(client_init.version).into());
        }

        // Verify auth token
        if !verifier.verify(
            &client_init.client_id,
            &client_init.auth_token,
            client_init.timestamp,
        ) {
            // Return auth failure via ServerInit
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

        // Generate session ID
        let session_id = Uuid::new_v4();

        // Derive preliminary key (for encrypting ServerInit)
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
        let ticket_nonce = [0u8; NONCE_SIZE]; // Static nonce is OK since ticket_key is unique per server
        let encrypted_ticket = ticket_cipher
            .encrypt(&ticket_nonce, &ticket_plaintext, &[])
            .map_err(|e| CryptoError::EncryptionFailed(format!("Ticket encrypt: {}", e)))?;

        // Build ServerInit
        let server_init = ServerInit {
            status: AcceptStatus::Ok,
            session_id,
            server_ephemeral_pub: server_pub,
            challenge,
            padding_min: padding_range.min,
            padding_max: padding_range.max,
            server_features,
            session_ticket: encrypted_ticket,
            padding: crate::crypto::padding::generate_padding(64),
        };
        let init_plaintext = encode_server_init(&server_init);

        // Encrypt ServerInit with preliminary key
        let prelim_cipher = create_cipher(CipherSuite::ChaCha20Poly1305, &prelim_key);
        let init_nonce = [0u8; NONCE_SIZE];
        let encrypted_init = encrypt_frame(prelim_cipher.as_ref(), &init_nonce, &init_plaintext)
            .map_err(|e| CryptoError::EncryptionFailed(format!("ServerInit encrypt: {}", e)))?;

        // Wire format: [server_pub:32][encrypted_frame]
        let mut wire = Vec::with_capacity(32 + encrypted_init.len());
        wire.extend_from_slice(&server_pub);
        wire.extend_from_slice(&encrypted_init);

        let state = ServerAwaitingChallengeV3 {
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

/// v3 server state: waiting for client's challenge response in first data frame.
pub struct ServerAwaitingChallengeV3 {
    pub session_key: [u8; 32],
    pub challenge: [u8; 32],
    pub session_id: Uuid,
    pub client_id: ClientId,
    pub cipher_suite: CipherSuite,
    pub padding_range: PaddingRange,
}

impl ServerAwaitingChallengeV3 {
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
            protocol_version: PROTOCOL_VERSION,
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

// ===== v1/v2 Handshake (4-step: ClientHello → ServerHello → ClientAuth → ServerAccept) =====

/// Client-side handshake state machine (v1/v2).
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
            version: PROTOCOL_VERSION_V2,
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
        let auth_token =
            util::compute_auth_token(&self.auth_secret, &self.client_id, self.timestamp);

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

        // If server included padding_range, this is a v2 session
        let (protocol_version, padding_range) = match accept.padding_range {
            Some(pr) => (PROTOCOL_VERSION_V2, pr),
            None => (PROTOCOL_VERSION_V1, PaddingRange::new(0, 0)),
        };

        Ok(SessionKeys {
            session_key: self.session_key,
            cipher_suite: self.cipher_suite,
            session_id: accept.session_id,
            client_id: self.client_id,
            client_nonce_counter: 0,
            server_nonce_counter: 0,
            protocol_version,
            padding_range,
            challenge: None,
            session_ticket: None,
        })
    }
}

/// Server-side handshake state machine (v1/v2).
pub struct ServerHandshake;

pub struct ServerAwaitingClientAuth {
    session_key: [u8; 32],
    challenge: Vec<u8>,
    #[allow(dead_code)]
    server_pub: [u8; 32],
    timestamp: u64,
    /// Protocol version from the client's ClientHello.
    client_version: u8,
    /// Padding range to negotiate (only used for v2+).
    padding_range: PaddingRange,
}

/// Callback for the server to verify client credentials.
pub trait AuthVerifier: Send + Sync {
    fn verify(&self, client_id: &ClientId, auth_token: &[u8; 32], timestamp: u64) -> bool;
}

impl ServerHandshake {
    /// Step 2: Process ClientHello, generate ServerHello.
    /// Accepts both v1 and v2 clients for backward compatibility.
    pub fn process_client_hello(
        client_hello_bytes: &[u8],
    ) -> Result<(Vec<u8>, ServerAwaitingClientAuth), PrismaError> {
        Self::process_client_hello_with_padding(client_hello_bytes, DEFAULT_PADDING_RANGE)
    }

    /// Step 2 with configurable padding range.
    pub fn process_client_hello_with_padding(
        client_hello_bytes: &[u8],
        padding_range: PaddingRange,
    ) -> Result<(Vec<u8>, ServerAwaitingClientAuth), PrismaError> {
        let client_hello = decode_client_hello(client_hello_bytes)?;

        // Accept v1 and v2 clients (v3 uses ClientInit, not ClientHello)
        if client_hello.version != PROTOCOL_VERSION_V2
            && client_hello.version != PROTOCOL_VERSION_V1
        {
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
            client_version: client_hello.version,
            padding_range,
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

        // Build ServerAccept — include padding_range for v2 clients
        let session_id = Uuid::new_v4();
        let is_v2 = self.client_version >= PROTOCOL_VERSION_V2;
        let accept = ServerAccept {
            status: AcceptStatus::Ok,
            session_id,
            padding_range: if is_v2 {
                Some(self.padding_range)
            } else {
                None
            },
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
            protocol_version: self.client_version,
            padding_range: if is_v2 {
                self.padding_range
            } else {
                PaddingRange::new(0, 0)
            },
            challenge: None,
            session_ticket: None,
        };

        Ok((encrypted_accept, session_keys))
    }
}

// ===== Version detection helper =====

/// Detect the protocol version from the first byte of a ClientHello/ClientInit.
/// v1/v2: ClientHello starts with version byte (0x01 or 0x02)
/// v3: ClientInit starts with version byte (0x03)
pub fn detect_protocol_version(first_byte: u8) -> u8 {
    first_byte
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
    fn test_full_handshake_v2() {
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
        assert_eq!(client_session.protocol_version, PROTOCOL_VERSION_V2);
    }

    #[test]
    fn test_handshake_bad_auth_v2() {
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

    #[test]
    fn test_v3_handshake() {
        let client_id = ClientId::new();
        let auth_secret = [0x42u8; 32];
        let ticket_key = [0xFFu8; 32];

        let verifier = TestVerifier {
            expected_id: client_id,
            auth_secret,
        };

        // Client step 1: send ClientInit
        let client_hs =
            ClientHandshakeV3::new(client_id, auth_secret, CipherSuite::ChaCha20Poly1305);
        let (client_state, client_init_bytes) = client_hs.start();

        // Server step 1: process ClientInit, produce ServerInit
        let padding_range = PaddingRange::new(0, 256);
        let (server_init_bytes, server_state) = ServerHandshakeV3::process_client_init(
            &client_init_bytes,
            padding_range,
            FEATURE_UDP_RELAY | FEATURE_SPEED_TEST,
            &ticket_key,
            &verifier,
        )
        .unwrap();

        // Client step 2: process ServerInit
        let client_session = client_state
            .process_server_init(&server_init_bytes)
            .unwrap();

        // Server produces session keys
        let server_session = server_state.into_session_keys();

        // Both sides should have the same session key
        assert_eq!(client_session.session_key, server_session.session_key);
        assert_eq!(client_session.session_id, server_session.session_id);
        assert_eq!(client_session.cipher_suite, server_session.cipher_suite);
        assert_eq!(client_session.protocol_version, PROTOCOL_VERSION);

        // Client should have a challenge to respond to
        assert!(client_session.challenge.is_some());
        assert!(client_session.session_ticket.is_some());

        // Verify challenge response
        let challenge = client_session.challenge.unwrap();
        let response_hash: [u8; 32] = blake3::hash(&challenge).into();
        assert!(server_state_verify_challenge(
            &server_session.challenge.unwrap(),
            &response_hash
        ));
    }

    fn server_state_verify_challenge(challenge: &[u8; 32], response_hash: &[u8; 32]) -> bool {
        let expected: [u8; 32] = blake3::hash(challenge).into();
        util::ct_eq(response_hash, &expected)
    }

    #[test]
    fn test_v3_handshake_bad_auth() {
        let client_id = ClientId::new();
        let client_secret = [0x42u8; 32];
        let wrong_secret = [0x99u8; 32];
        let ticket_key = [0xFFu8; 32];

        let verifier = TestVerifier {
            expected_id: client_id,
            auth_secret: wrong_secret,
        };

        let client_hs =
            ClientHandshakeV3::new(client_id, client_secret, CipherSuite::ChaCha20Poly1305);
        let (_, client_init_bytes) = client_hs.start();

        let result = ServerHandshakeV3::process_client_init(
            &client_init_bytes,
            PaddingRange::new(0, 256),
            0,
            &ticket_key,
            &verifier,
        );
        assert!(result.is_err());
    }
}
