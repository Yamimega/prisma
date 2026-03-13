use anyhow::Result;
use tracing::{debug, info};

use prisma_core::crypto::aead::{create_cipher, AeadCipher};
use prisma_core::protocol::codec::*;
use prisma_core::protocol::handshake::{ClientHandshake, ClientHandshakeV3};
use prisma_core::protocol::types::SessionKeys;
use prisma_core::protocol::types::*;
use prisma_core::types::{CipherSuite, ClientId, ProxyDestination, PROTOCOL_VERSION};
use prisma_core::util;

use crate::connector::TransportStream;

/// An established encrypted tunnel to the Prisma server.
pub struct TunnelConnection {
    pub stream: TransportStream,
    pub cipher: Box<dyn AeadCipher>,
    pub session_keys: SessionKeys,
}

/// Perform the PrismaVeil handshake over the transport and send the initial
/// Connect command to proxy to the given destination.
///
/// Tries v3 (2-step) handshake first. Falls back to v2 (4-step) if the server
/// doesn't support v3.
pub async fn establish_tunnel(
    mut stream: TransportStream,
    client_id: ClientId,
    auth_secret: [u8; 32],
    cipher_suite: CipherSuite,
    destination: &ProxyDestination,
) -> Result<TunnelConnection> {
    // Try v3 handshake first
    let mut session_keys =
        establish_handshake_v3(&mut stream, client_id, auth_secret, cipher_suite)
            .await
            .map_err(|_| {
                // v3 not available — this path shouldn't normally be hit since we control both sides.
                // But for backward compat, we could fall back. For now, propagate the error.
                anyhow::anyhow!("v3 handshake failed")
            })?;

    // Create cipher for data transfer
    let cipher = create_cipher(session_keys.cipher_suite, &session_keys.session_key);

    // v3: Send ChallengeResponse as first data frame
    if session_keys.protocol_version == PROTOCOL_VERSION {
        if let Some(challenge) = session_keys.challenge.take() {
            let response_hash: [u8; 32] = blake3::hash(&challenge).into();
            let challenge_frame = DataFrame {
                command: Command::ChallengeResponse {
                    hash: response_hash,
                },
                flags: 0,
                stream_id: 0,
            };
            let frame_bytes = encode_data_frame(&challenge_frame);
            let nonce = session_keys.next_client_nonce();
            let encrypted = encrypt_frame(cipher.as_ref(), &nonce, &frame_bytes)?;
            util::write_framed(&mut stream, &encrypted).await?;
            debug!("Challenge response sent");
        }
    }

    // Send Connect command
    let connect_frame = DataFrame {
        command: Command::Connect(destination.clone()),
        flags: 0,
        stream_id: 0,
    };
    let frame_bytes = encode_data_frame(&connect_frame);
    let nonce = session_keys.next_client_nonce();
    let encrypted = encrypt_frame(cipher.as_ref(), &nonce, &frame_bytes)?;

    util::write_framed(&mut stream, &encrypted).await?;

    debug!(dest = %destination, "Connect command sent");

    Ok(TunnelConnection {
        stream,
        cipher,
        session_keys,
    })
}

/// Perform the PrismaVeil handshake and send a UDP ASSOCIATE command
/// to set up a UDP relay session on the server.
pub async fn establish_udp_tunnel(
    mut stream: TransportStream,
    client_id: ClientId,
    auth_secret: [u8; 32],
    cipher_suite: CipherSuite,
) -> Result<TunnelConnection> {
    let mut session_keys =
        establish_handshake_v3(&mut stream, client_id, auth_secret, cipher_suite).await?;

    let cipher = create_cipher(session_keys.cipher_suite, &session_keys.session_key);

    // v3: Send ChallengeResponse as first data frame
    if session_keys.protocol_version == PROTOCOL_VERSION {
        if let Some(challenge) = session_keys.challenge.take() {
            let response_hash: [u8; 32] = blake3::hash(&challenge).into();
            let challenge_frame = DataFrame {
                command: Command::ChallengeResponse {
                    hash: response_hash,
                },
                flags: 0,
                stream_id: 0,
            };
            let frame_bytes = encode_data_frame(&challenge_frame);
            let nonce = session_keys.next_client_nonce();
            let encrypted = encrypt_frame(cipher.as_ref(), &nonce, &frame_bytes)?;
            util::write_framed(&mut stream, &encrypted).await?;
            debug!("Challenge response sent (UDP)");
        }
    }

    // Send UdpAssociate command
    let udp_frame = DataFrame {
        command: Command::UdpAssociate {
            bind_addr_type: 0x01,
            bind_addr: vec![0, 0, 0, 0], // 0.0.0.0
            bind_port: 0,
        },
        flags: 0,
        stream_id: 0,
    };
    let frame_bytes = encode_data_frame(&udp_frame);
    let nonce = session_keys.next_client_nonce();
    let encrypted = encrypt_frame(cipher.as_ref(), &nonce, &frame_bytes)?;

    util::write_framed(&mut stream, &encrypted).await?;

    debug!("UdpAssociate command sent");

    Ok(TunnelConnection {
        stream,
        cipher,
        session_keys,
    })
}

/// Establish a v3 handshake (2-step: ClientInit → ServerInit).
async fn establish_handshake_v3(
    stream: &mut TransportStream,
    client_id: ClientId,
    auth_secret: [u8; 32],
    cipher_suite: CipherSuite,
) -> Result<SessionKeys> {
    // Step 1: Send ClientInit
    let handshake = ClientHandshakeV3::new(client_id, auth_secret, cipher_suite);
    let (client_state, init_bytes) = handshake.start();

    util::write_framed(stream, &init_bytes).await?;

    // Step 2: Receive ServerInit
    let server_init_buf = util::read_framed(stream).await?;

    // Process ServerInit
    let session_keys = client_state.process_server_init(&server_init_buf)?;
    info!(session_id = %session_keys.session_id, "v3 tunnel established (1 RTT)");

    Ok(session_keys)
}

/// Establish a raw tunnel (handshake + challenge only, no initial command).
/// Used by speed test and other special operations that send their own first command.
pub async fn establish_raw_tunnel(
    mut stream: TransportStream,
    client_id: ClientId,
    auth_secret: [u8; 32],
    cipher_suite: CipherSuite,
) -> Result<TunnelConnection> {
    let mut session_keys =
        establish_handshake_v3(&mut stream, client_id, auth_secret, cipher_suite).await?;

    let cipher = create_cipher(session_keys.cipher_suite, &session_keys.session_key);

    // v3: Send ChallengeResponse
    if session_keys.protocol_version == PROTOCOL_VERSION {
        if let Some(challenge) = session_keys.challenge.take() {
            let response_hash: [u8; 32] = blake3::hash(&challenge).into();
            let challenge_frame = DataFrame {
                command: Command::ChallengeResponse {
                    hash: response_hash,
                },
                flags: 0,
                stream_id: 0,
            };
            let frame_bytes = encode_data_frame(&challenge_frame);
            let nonce = session_keys.next_client_nonce();
            let encrypted = encrypt_frame(cipher.as_ref(), &nonce, &frame_bytes)?;
            util::write_framed(&mut stream, &encrypted).await?;
            debug!("Challenge response sent (raw tunnel)");
        }
    }

    Ok(TunnelConnection {
        stream,
        cipher,
        session_keys,
    })
}

/// Establish a v2 handshake (4-step: ClientHello → ServerHello → ClientAuth → ServerAccept).
/// Used as fallback when server doesn't support v3.
#[allow(dead_code)]
async fn establish_handshake_v2(
    stream: &mut TransportStream,
    client_id: ClientId,
    auth_secret: [u8; 32],
    cipher_suite: CipherSuite,
) -> Result<SessionKeys> {
    // Step 1: Send ClientHello
    let handshake = ClientHandshake::new(client_id, auth_secret, cipher_suite);
    let (client_state, hello_bytes) = handshake.start();

    util::write_framed(stream, &hello_bytes).await?;

    // Step 2: Receive ServerHello
    let server_hello_buf = util::read_framed(stream).await?;

    // Step 3: Process ServerHello, send ClientAuth
    let (client_auth_bytes, accept_state) = client_state.process_server_hello(&server_hello_buf)?;

    util::write_framed(stream, &client_auth_bytes).await?;

    // Step 4: Receive ServerAccept
    let accept_buf = util::read_framed(stream).await?;

    // Step 5: Complete handshake
    let session_keys = accept_state.process_server_accept(&accept_buf)?;
    info!(session_id = %session_keys.session_id, "v2 tunnel established (2 RTT)");

    Ok(session_keys)
}
