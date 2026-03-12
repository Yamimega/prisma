use anyhow::Result;
use prisma_core::types::{CipherSuite, ClientId};

use crate::connector::{self, TransportStream};

/// Shared configuration for all proxy sessions (SOCKS5 and HTTP).
#[derive(Clone)]
pub struct ProxyContext {
    pub server_addr: String,
    pub client_id: ClientId,
    pub auth_secret: [u8; 32],
    pub cipher_suite: CipherSuite,
    pub use_quic: bool,
    pub skip_cert_verify: bool,
    pub tls_on_tcp: bool,
    pub alpn_protocols: Vec<String>,
    pub tls_server_name: Option<String>,
}

impl ProxyContext {
    /// Connect to the remote Prisma server using the configured transport.
    pub async fn connect(&self) -> Result<TransportStream> {
        let server_name = self.server_name();
        if self.use_quic {
            connector::connect_quic(&self.server_addr, self.skip_cert_verify, &self.alpn_protocols, server_name).await
        } else if self.tls_on_tcp {
            connector::connect_tcp_tls(&self.server_addr, server_name, self.skip_cert_verify, &self.alpn_protocols).await
        } else {
            connector::connect_tcp(&self.server_addr).await
        }
    }

    /// Resolve the server name for TLS SNI.
    /// Uses `tls_server_name` if set, otherwise extracts hostname from `server_addr`.
    pub fn server_name(&self) -> &str {
        if let Some(ref name) = self.tls_server_name {
            return name;
        }
        // Extract hostname from "host:port"
        self.server_addr
            .split(':')
            .next()
            .unwrap_or("prisma-server")
    }
}
