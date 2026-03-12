use prisma_core::types::{CipherSuite, ClientId};

/// Shared configuration for all proxy sessions (SOCKS5 and HTTP).
#[derive(Clone)]
pub struct ProxyContext {
    pub server_addr: String,
    pub client_id: ClientId,
    pub auth_secret: [u8; 32],
    pub cipher_suite: CipherSuite,
    pub use_quic: bool,
    pub skip_cert_verify: bool,
}
