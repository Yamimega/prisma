use std::sync::Arc;

use anyhow::Result;
use prisma_core::congestion::CongestionMode;
use prisma_core::dns::DnsConfig;
use prisma_core::fec::FecConfig;
use prisma_core::router::Router;
use prisma_core::types::{CipherSuite, ClientId, QUIC_ALPN};
use tracing::{info, warn};

use crate::connector::{self, TransportStream};
use crate::dns_resolver::DnsResolver;

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
    pub use_ws: bool,
    pub ws_url: Option<String>,
    pub ws_extra_headers: Vec<(String, String)>,
    pub use_grpc: bool,
    pub grpc_url: Option<String>,
    pub use_xhttp: bool,
    pub xhttp_mode: Option<String>,
    pub xhttp_stream_url: Option<String>,
    pub xhttp_upload_url: Option<String>,
    pub xhttp_download_url: Option<String>,
    pub xhttp_extra_headers: Vec<(String, String)>,
    pub user_agent: Option<String>,
    pub referer: Option<String>,
    pub congestion_mode: CongestionMode,
    pub port_hopping: prisma_core::port_hop::PortHoppingConfig,
    pub salamander_password: Option<String>,
    pub udp_fec: Option<FecConfig>,
    pub dns_config: DnsConfig,
    pub dns_resolver: DnsResolver,
    pub router: Arc<Router>,
}

impl ProxyContext {
    /// Connect to the remote Prisma server using the configured transport.
    pub async fn connect(&self) -> Result<TransportStream> {
        let transport = if self.use_xhttp {
            "XHTTP"
        } else if self.use_ws {
            "WebSocket"
        } else if self.use_grpc {
            "gRPC"
        } else if self.use_quic {
            "QUIC"
        } else if self.tls_on_tcp {
            "TLS-on-TCP"
        } else {
            "TCP"
        };

        // When QUIC is used without camouflage, the server expects the native ALPN.
        // The configured alpn_protocols (default ["h2","http/1.1"]) are only for camouflage mode.
        let default_quic_alpn = vec![QUIC_ALPN.to_string()];
        let alpn = if self.use_quic && !self.tls_on_tcp {
            &default_quic_alpn
        } else {
            &self.alpn_protocols
        };

        let result = if self.use_xhttp {
            let stream_url = self
                .xhttp_stream_url
                .as_deref()
                .unwrap_or("https://localhost/api/v1/stream");
            connector::connect_xhttp(
                stream_url,
                &self.xhttp_extra_headers,
                self.user_agent.as_deref(),
                self.referer.as_deref(),
            )
            .await
        } else if self.use_ws {
            let ws_url = self.ws_url.as_deref().unwrap_or("ws://localhost");
            connector::connect_ws(ws_url, self.skip_cert_verify, &self.ws_extra_headers).await
        } else if self.use_grpc {
            let grpc_url = self.grpc_url.as_deref().unwrap_or("http://localhost");
            connector::connect_grpc(grpc_url).await
        } else if self.use_quic {
            // Apply port hopping if enabled
            let server_addr = if self.port_hopping.enabled {
                let port = prisma_core::port_hop::current_port(
                    &self.port_hopping,
                    &self.auth_secret,
                    std::time::SystemTime::now(),
                );
                // Replace port in server address
                let host = self
                    .server_addr
                    .split(':')
                    .next()
                    .unwrap_or(&self.server_addr);
                format!("{}:{}", host, port)
            } else {
                self.server_addr.clone()
            };
            connector::connect_quic(
                &server_addr,
                self.skip_cert_verify,
                alpn,
                self.server_name(),
                &self.congestion_mode,
                self.salamander_password.as_deref(),
            )
            .await
        } else if self.tls_on_tcp {
            connector::connect_tcp_tls(
                &self.server_addr,
                self.server_name(),
                self.skip_cert_verify,
                &self.alpn_protocols,
            )
            .await
        } else {
            connector::connect_tcp(&self.server_addr).await
        };

        match &result {
            Ok(_) => info!(
                server = %self.server_addr,
                transport = %transport,
                "Connected to server"
            ),
            Err(e) => warn!(
                server = %self.server_addr,
                transport = %transport,
                error = %e,
                "Failed to connect to server"
            ),
        }
        result
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
