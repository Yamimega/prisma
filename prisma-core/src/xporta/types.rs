use serde::{Deserialize, Serialize};

/// Encoding mode for XPorta payloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XPortaEncoding {
    /// JSON with base64 payload — maximum stealth, ~37% overhead.
    Json,
    /// Binary framing — maximum throughput, ~0.5% overhead.
    Binary,
}

impl XPortaEncoding {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "json" => Some(Self::Json),
            "binary" => Some(Self::Binary),
            _ => None,
        }
    }

    pub fn content_type(&self) -> &'static str {
        match self {
            Self::Json => "application/json",
            Self::Binary => "application/octet-stream",
        }
    }
}

/// Session initialization request (client → server).
#[derive(Debug, Serialize, Deserialize)]
pub struct SessionInitRequest {
    /// Protocol version.
    pub v: u8,
    /// Timestamp (Unix seconds).
    pub t: u64,
    /// Client ID hex.
    pub c: String,
    /// HMAC-SHA256 auth token hex.
    pub a: String,
    /// Random padding.
    #[serde(default)]
    pub p: String,
}

/// Upload request — JSON mode.
#[derive(Debug, Serialize, Deserialize)]
pub struct UploadRequest {
    /// Sequence number.
    pub s: u32,
    /// Base64-encoded payload.
    pub d: String,
    /// Random padding.
    #[serde(default)]
    pub p: String,
}

/// Upload response — JSON mode (may piggyback download data).
#[derive(Debug, Serialize, Deserialize)]
pub struct UploadResponse {
    pub ok: bool,
    /// Download sequence (if piggyback data present).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub s: Option<u32>,
    /// Base64-encoded download data (piggyback).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub d: Option<String>,
    /// Random padding.
    #[serde(default)]
    pub p: String,
}

/// A single download item in a poll response.
#[derive(Debug, Serialize, Deserialize)]
pub struct PollItem {
    /// Sequence number.
    pub s: u32,
    /// Base64-encoded payload.
    pub d: String,
}

/// Poll response — JSON mode.
#[derive(Debug, Serialize, Deserialize)]
pub struct PollResponse {
    pub items: Vec<PollItem>,
    /// Random padding.
    #[serde(default)]
    pub p: String,
}

/// Binary upload frame: [seq:4 LE][payload_len:4 LE][payload][padding]
#[derive(Debug)]
pub struct BinaryUploadFrame {
    pub seq: u32,
    pub payload: Vec<u8>,
}

/// Binary download frame: [seq:4 LE][payload_len:4 LE][payload]
#[derive(Debug)]
pub struct BinaryDownloadFrame {
    pub seq: u32,
    pub payload: Vec<u8>,
}

/// Error JSON returned for unauthenticated requests.
#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
    pub code: u16,
}

/// Maximum number of out-of-order entries in the reassembler.
pub const REASSEMBLER_MAX_BUFFER: usize = 64;

/// Gap timeout in seconds — if a gap persists longer, treat as connection error.
pub const REASSEMBLER_GAP_TIMEOUT_SECS: u64 = 10;

/// Default poll timeout in seconds (must be < Cloudflare's 100s limit).
pub const DEFAULT_POLL_TIMEOUT_SECS: u16 = 55;

/// Default poll concurrency.
pub const DEFAULT_POLL_CONCURRENCY: u8 = 3;

/// Default upload concurrency.
pub const DEFAULT_UPLOAD_CONCURRENCY: u8 = 4;

/// Default max payload size.
pub const DEFAULT_MAX_PAYLOAD_SIZE: u32 = 65536;

/// Default session timeout in seconds.
pub const DEFAULT_SESSION_TIMEOUT_SECS: u64 = 300;
