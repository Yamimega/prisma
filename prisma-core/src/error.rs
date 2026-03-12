use thiserror::Error;

/// Top-level error type for the Prisma proxy suite.
#[derive(Debug, Error)]
pub enum PrismaError {
    #[error("protocol error: {0}")]
    Protocol(#[from] ProtocolError),

    #[error("crypto error: {0}")]
    Crypto(#[from] CryptoError),

    #[error("config error: {0}")]
    Config(#[from] ConfigError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("connection error: {0}")]
    Connection(String),

    #[error("auth error: {0}")]
    Auth(String),

    #[error("{0}")]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error("invalid protocol version: {0}")]
    InvalidVersion(u8),

    #[error("invalid command: {0}")]
    InvalidCommand(u8),

    #[error("invalid address type: {0}")]
    InvalidAddressType(u8),

    #[error("frame too large: {size} > {max}")]
    FrameTooLarge { size: usize, max: usize },

    #[error("unexpected message in state {state}: {message}")]
    UnexpectedMessage { state: String, message: String },

    #[error("handshake failed: {0}")]
    HandshakeFailed(String),

    #[error("replay detected: nonce {0}")]
    ReplayDetected(u64),

    #[error("invalid frame: {0}")]
    InvalidFrame(String),
}

#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("encryption failed: {0}")]
    EncryptionFailed(String),

    #[error("decryption failed: {0}")]
    DecryptionFailed(String),

}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("invalid config: {0}")]
    Invalid(String),

    #[error("validation failed: {0}")]
    ValidationFailed(String),

    #[error("parse error: {0}")]
    ParseError(String),
}

pub type Result<T> = std::result::Result<T, PrismaError>;
