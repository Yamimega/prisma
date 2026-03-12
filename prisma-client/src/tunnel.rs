use anyhow::Result;
use tracing::{debug, info};

use prisma_core::crypto::aead::{create_cipher, AeadCipher};
use prisma_core::protocol::codec::*;
use prisma_core::protocol::handshake::ClientHandshake;
use prisma_core::protocol::types::SessionKeys;
use prisma_core::protocol::types::*;
use prisma_core::types::{CipherSuite, ClientId, ProxyDestination};
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
pub async fn establish_tunnel(
    mut stream: TransportStream,
    client_id: ClientId,
    auth_secret: [u8; 32],
    cipher_suite: CipherSuite,
    destination: &ProxyDestination,
) -> Result<TunnelConnection> {
    // Step 1: Send ClientHello
    let handshake = ClientHandshake::new(client_id, auth_secret, cipher_suite);
    let (client_state, hello_bytes) = handshake.start();

    util::write_framed(&mut stream, &hello_bytes).await?;

    // Step 2: Receive ServerHello
    let server_hello_buf = util::read_framed(&mut stream).await?;

    // Step 3: Process ServerHello, send ClientAuth
    let (client_auth_bytes, accept_state) = client_state.process_server_hello(&server_hello_buf)?;

    util::write_framed(&mut stream, &client_auth_bytes).await?;

    // Step 4: Receive ServerAccept
    let accept_buf = util::read_framed(&mut stream).await?;

    // Step 5: Complete handshake
    let mut session_keys = accept_state.process_server_accept(&accept_buf)?;
    info!(session_id = %session_keys.session_id, "Tunnel established");

    // Create cipher for data transfer
    let cipher = create_cipher(session_keys.cipher_suite, &session_keys.session_key);

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
