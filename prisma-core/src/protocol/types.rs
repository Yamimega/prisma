use uuid::Uuid;

use crate::types::{CipherSuite, ClientId, PaddingRange, ProxyDestination};

// Command byte values
pub const CMD_CONNECT: u8 = 0x01;
pub const CMD_DATA: u8 = 0x02;
pub const CMD_CLOSE: u8 = 0x03;
pub const CMD_PING: u8 = 0x04;
pub const CMD_PONG: u8 = 0x05;
pub const CMD_REGISTER_FORWARD: u8 = 0x06;
pub const CMD_FORWARD_READY: u8 = 0x07;
pub const CMD_FORWARD_CONNECT: u8 = 0x08;
// v3 commands
pub const CMD_UDP_ASSOCIATE: u8 = 0x09;
pub const CMD_UDP_DATA: u8 = 0x0A;
pub const CMD_SPEED_TEST: u8 = 0x0B;
pub const CMD_DNS_QUERY: u8 = 0x0C;
pub const CMD_DNS_RESPONSE: u8 = 0x0D;
pub const CMD_CHALLENGE_RESP: u8 = 0x0E;

// Flag bits (v3: 2-byte little-endian flags)
pub const FLAG_PADDED: u16 = 0x0001;
pub const FLAG_FEC: u16 = 0x0002;
pub const FLAG_PRIORITY: u16 = 0x0004;
pub const FLAG_DATAGRAM: u16 = 0x0008;
pub const FLAG_COMPRESSED: u16 = 0x0010;
pub const FLAG_0RTT: u16 = 0x0020;

// Legacy v2 flag for backward compat
pub const FLAG_PADDED_V2: u8 = 0x01;

/// Server feature flags bitmask (v3 ServerInit).
pub const FEATURE_UDP_RELAY: u32 = 0x0001;
pub const FEATURE_FEC: u32 = 0x0002;
pub const FEATURE_PORT_HOPPING: u32 = 0x0004;
pub const FEATURE_SPEED_TEST: u32 = 0x0008;
pub const FEATURE_DNS_TUNNEL: u32 = 0x0010;
pub const FEATURE_BANDWIDTH_LIMIT: u32 = 0x0020;

/// Handshake: Client → Server (v1/v2 step 1)
#[derive(Debug, Clone)]
pub struct ClientHello {
    pub version: u8,
    pub client_ephemeral_pub: [u8; 32],
    pub timestamp: u64,
    pub padding: Vec<u8>,
}

/// Handshake: Server → Client (v1/v2 step 2)
#[derive(Debug, Clone)]
pub struct ServerHello {
    pub server_ephemeral_pub: [u8; 32],
    pub encrypted_challenge: Vec<u8>, // Encrypted with derived session key
    pub padding: Vec<u8>,
}

/// Handshake: Client → Server (v1/v2 step 3) — encrypted with session key
#[derive(Debug, Clone)]
pub struct ClientAuth {
    pub client_id: ClientId,
    pub auth_token: [u8; 32], // HMAC-SHA256
    pub cipher_suite: CipherSuite,
    pub challenge_response: [u8; 32], // BLAKE3 hash of challenge
}

/// Handshake: Server → Client (v1/v2 step 4)
#[derive(Debug, Clone)]
pub struct ServerAccept {
    pub status: AcceptStatus,
    pub session_id: Uuid,
    /// Negotiated padding range for per-frame padding (v2 only).
    pub padding_range: Option<PaddingRange>,
}

// --- v3 handshake types ---

/// v3 Handshake: Client → Server (step 1)
/// Combines ClientHello + ClientAuth into a single message.
#[derive(Debug, Clone)]
pub struct ClientInit {
    pub version: u8, // 0x03
    pub flags: u8,   // bit0: has_0rtt_data, bit1: resumption
    pub client_ephemeral_pub: [u8; 32],
    pub client_id: ClientId, // UUID (16 bytes)
    pub timestamp: u64,      // Unix timestamp (seconds)
    pub cipher_suite: CipherSuite,
    pub auth_token: [u8; 32], // HMAC-SHA256(auth_secret, client_id || timestamp)
    pub padding: Vec<u8>,
}

/// v3 Handshake: Server → Client (step 1 response)
/// Combines ServerHello + ServerAccept into a single encrypted message.
#[derive(Debug, Clone)]
pub struct ServerInit {
    pub status: AcceptStatus,
    pub session_id: Uuid,
    pub server_ephemeral_pub: [u8; 32],
    pub challenge: [u8; 32], // Random challenge
    pub padding_min: u16,
    pub padding_max: u16,
    pub server_features: u32,    // Bitmask of supported features
    pub session_ticket: Vec<u8>, // Opaque ticket for 0-RTT resumption
    pub padding: Vec<u8>,
}

/// v3 0-RTT Resumption: Client → Server
#[derive(Debug, Clone)]
pub struct ClientResume {
    pub version: u8, // 0x03
    pub flags: u8,   // bit1=1 (resumption)
    pub client_ephemeral_pub: [u8; 32],
    pub session_ticket: Vec<u8>,
    pub encrypted_0rtt_data: Vec<u8>,
}

/// Opaque session ticket for 0-RTT resumption.
/// Server encrypts this with a server-side ticket key.
/// Plaintext contents:
///   [client_id:16][session_key:32][cipher_suite:1][issued_at:8][padding_min:2][padding_max:2]
#[derive(Debug, Clone)]
pub struct SessionTicket {
    pub client_id: ClientId,
    pub session_key: [u8; 32],
    pub cipher_suite: CipherSuite,
    pub issued_at: u64,
    pub padding_range: PaddingRange,
}

/// ClientInit flags
pub const CLIENT_INIT_FLAG_0RTT: u8 = 0x01;
pub const CLIENT_INIT_FLAG_RESUMPTION: u8 = 0x02;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum AcceptStatus {
    Ok = 0x00,
    AuthFailed = 0x01,
    ServerBusy = 0x02,
    VersionMismatch = 0x03,
    QuotaExceeded = 0x04,
}

impl AcceptStatus {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0x00 => Some(AcceptStatus::Ok),
            0x01 => Some(AcceptStatus::AuthFailed),
            0x02 => Some(AcceptStatus::ServerBusy),
            0x03 => Some(AcceptStatus::VersionMismatch),
            0x04 => Some(AcceptStatus::QuotaExceeded),
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
    RegisterForward {
        remote_port: u16,
        name: String,
    },
    /// Server → Client: acknowledge a port forward registration.
    ForwardReady {
        remote_port: u16,
        success: bool,
    },
    /// Server → Client: a new inbound connection arrived on a forwarded port.
    /// The `stream_id` on the enclosing DataFrame identifies this forwarded connection.
    ForwardConnect {
        remote_port: u16,
    },
    // v3 commands
    /// Client → Server: set up UDP relay session.
    UdpAssociate {
        bind_addr_type: u8,
        bind_addr: Vec<u8>,
        bind_port: u16,
    },
    /// Both: UDP datagram relay.
    UdpData {
        assoc_id: u32,
        frag: u8,
        addr_type: u8,
        dest_addr: Vec<u8>,
        dest_port: u16,
        payload: Vec<u8>,
    },
    /// Both: bandwidth measurement.
    SpeedTest {
        direction: u8, // 0=download, 1=upload
        duration_secs: u8,
        data: Vec<u8>,
    },
    /// Client → Server: encrypted DNS query.
    DnsQuery {
        query_id: u16,
        data: Vec<u8>,
    },
    /// Server → Client: encrypted DNS response.
    DnsResponse {
        query_id: u16,
        data: Vec<u8>,
    },
    /// Client → Server: challenge response (first frame after v3 handshake).
    ChallengeResponse {
        hash: [u8; 32], // BLAKE3(challenge)
    },
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
            Command::UdpAssociate { .. } => CMD_UDP_ASSOCIATE,
            Command::UdpData { .. } => CMD_UDP_DATA,
            Command::SpeedTest { .. } => CMD_SPEED_TEST,
            Command::DnsQuery { .. } => CMD_DNS_QUERY,
            Command::DnsResponse { .. } => CMD_DNS_RESPONSE,
            Command::ChallengeResponse { .. } => CMD_CHALLENGE_RESP,
        }
    }
}

/// A data frame in the PrismaVeil wire protocol.
/// v2 format: [cmd:1][flags:1][stream_id:4][payload:var][padding:var]
/// v3 format: [cmd:1][flags:2][stream_id:4][payload:var][padding:var]
#[derive(Debug, Clone)]
pub struct DataFrame {
    pub command: Command,
    pub flags: u16,
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
    /// Protocol version negotiated during handshake.
    pub protocol_version: u8,
    /// Padding range for per-frame padding (v2+ only).
    pub padding_range: PaddingRange,
    /// v3: challenge from ServerInit that client must respond to.
    pub challenge: Option<[u8; 32]>,
    /// v3: session ticket for 0-RTT resumption.
    pub session_ticket: Option<Vec<u8>>,
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
