use serde::{Deserialize, Serialize};

use super::server::LoggingConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientConfig {
    pub socks5_listen_addr: String,
    #[serde(default)]
    pub http_listen_addr: Option<String>,
    pub server_addr: String,
    pub identity: ClientIdentity,
    #[serde(default = "default_cipher_suite")]
    pub cipher_suite: String,
    #[serde(default = "default_transport")]
    pub transport: String,
    #[serde(default)]
    pub skip_cert_verify: bool,
    #[serde(default)]
    pub logging: LoggingConfig,
    #[serde(default)]
    pub port_forwards: Vec<PortForwardConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientIdentity {
    pub client_id: String,
    pub auth_secret: String, // hex-encoded
}

/// A port forwarding rule: expose a local service on the server's public port.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortForwardConfig {
    pub name: String,
    pub local_addr: String,
    pub remote_port: u16,
}

fn default_cipher_suite() -> String {
    "chacha20-poly1305".into()
}

fn default_transport() -> String {
    "quic".into()
}
