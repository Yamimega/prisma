use uuid::Uuid;

use crate::types::{CipherSuite, ClientId, ProxyDestination};

// Command byte values
pub const CMD_CONNECT: u8 = 0x01;
pub const CMD_DATA: u8 = 0x02;
pub const CMD_CLOSE: u8 = 0x03;
pub const CMD_PING: u8 = 0x04;
pub const CMD_PONG: u8 = 0x05;
pub const CMD_REGISTER_FORWARD: u8 = 0x06;
pub const CMD_FORWARD_READY: u8 = 0x07;
pub const CMD_FORWARD_CONNECT: u8 = 0x08;

// Flag bits
pub const FLAG_PADDED: u8 = 0x01;

/// Handshake: Client → Server (step 1)
#[derive(Debug, Clone)]
pub struct ClientHello {
    pub version: u8,
    pub client_ephemeral_pub: [u8; 32],
    pub timestamp: u64,
    pub padding: Vec<u8>,
}

/// Handshake: Server → Client (step 2)
#[derive(Debug, Clone)]
pub struct ServerHello {
    pub server_ephemeral_pub: [u8; 32],
    pub encrypted_challenge: Vec<u8>, // Encrypted with derived session key
    pub padding: Vec<u8>,
}

/// Handshake: Client → Server (step 3) — encrypted with session key
#[derive(Debug, Clone)]
pub struct ClientAuth {
    pub client_id: ClientId,
    pub auth_token: [u8; 32], // HMAC-SHA256
    pub cipher_suite: CipherSuite,
    pub challenge_response: [u8; 32], // BLAKE3 hash of challenge
}

/// Handshake: Server → Client (step 4)
#[derive(Debug, Clone)]
pub struct ServerAccept {
    pub status: AcceptStatus,
    pub session_id: Uuid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum AcceptStatus {
    Ok = 0x00,
    AuthFailed = 0x01,
    ServerBusy = 0x02,
    VersionMismatch = 0x03,
}

impl AcceptStatus {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0x00 => Some(AcceptStatus::Ok),
            0x01 => Some(AcceptStatus::AuthFailed),
            0x02 => Some(AcceptStatus::ServerBusy),
            0x03 => Some(AcceptStatus::VersionMismatch),
            _ => None,
        }
    }
}

/// Data command variants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Connect(ProxyDestination),
    Data(Vec<u8>),
    Close,
    Ping(u32),
    Pong(u32),
    /// Client → Server: request the server to listen on `remote_port` and forward traffic back.
    RegisterForward { remote_port: u16, name: String },
    /// Server → Client: acknowledge a port forward registration.
    ForwardReady { remote_port: u16, success: bool },
    /// Server → Client: a new inbound connection arrived on a forwarded port.
    /// The `stream_id` on the enclosing DataFrame identifies this forwarded connection.
    ForwardConnect { remote_port: u16 },
}

impl Command {
    pub fn cmd_byte(&self) -> u8 {
        match self {
            Command::Connect(_) => CMD_CONNECT,
            Command::Data(_) => CMD_DATA,
            Command::Close => CMD_CLOSE,
            Command::Ping(_) => CMD_PING,
            Command::Pong(_) => CMD_PONG,
            Command::RegisterForward { .. } => CMD_REGISTER_FORWARD,
            Command::ForwardReady { .. } => CMD_FORWARD_READY,
            Command::ForwardConnect { .. } => CMD_FORWARD_CONNECT,
        }
    }
}

/// A data frame in the PrismaVeil wire protocol.
/// Plaintext format: [cmd:1][flags:1][stream_id:4][payload:var][padding:var]
#[derive(Debug, Clone)]
pub struct DataFrame {
    pub command: Command,
    pub flags: u8,
    pub stream_id: u32,
}

/// Session keys produced after successful handshake.
#[derive(Debug, Clone)]
pub struct SessionKeys {
    pub session_key: [u8; 32],
    pub cipher_suite: CipherSuite,
    pub session_id: Uuid,
    pub client_id: ClientId,
    pub client_nonce_counter: u64,
    pub server_nonce_counter: u64,
}

impl SessionKeys {
    /// Generate the next nonce for client→server direction.
    pub fn next_client_nonce(&mut self) -> [u8; 12] {
        let nonce = nonce_from_counter(self.client_nonce_counter, true);
        self.client_nonce_counter += 1;
        nonce
    }

    /// Generate the next nonce for server→client direction.
    pub fn next_server_nonce(&mut self) -> [u8; 12] {
        let nonce = nonce_from_counter(self.server_nonce_counter, false);
        self.server_nonce_counter += 1;
        nonce
    }
}

/// Build a 12-byte nonce from a counter and direction flag.
/// Format: [direction:1][0:3][counter:8]
fn nonce_from_counter(counter: u64, is_client: bool) -> [u8; 12] {
    let mut nonce = [0u8; 12];
    nonce[0] = if is_client { 0x00 } else { 0x01 };
    nonce[4..12].copy_from_slice(&counter.to_be_bytes());
    nonce
}
