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
    #[serde(default)]
    pub tls_on_tcp: bool,
    #[serde(default = "super::default_alpn")]
    pub alpn_protocols: Vec<String>,
    #[serde(default)]
    pub tls_server_name: Option<String>,
    #[serde(default)]
    pub ws_url: Option<String>,
    #[serde(default)]
    pub ws_host: Option<String>,
    #[serde(default)]
    pub ws_extra_headers: Vec<(String, String)>,
    #[serde(default)]
    pub grpc_url: Option<String>,
    // XHTTP transport
    #[serde(default)]
    pub xhttp_mode: Option<String>,
    #[serde(default)]
    pub xhttp_upload_url: Option<String>,
    #[serde(default)]
    pub xhttp_download_url: Option<String>,
    #[serde(default)]
    pub xhttp_stream_url: Option<String>,
    #[serde(default)]
    pub xhttp_extra_headers: Vec<(String, String)>,
    // XMUX connection pool
    #[serde(default)]
    pub xmux: Option<XmuxConfig>,
    // Header obfuscation
    #[serde(default)]
    pub user_agent: Option<String>,
    #[serde(default)]
    pub referer: Option<String>,
    // Congestion control (QUIC only)
    #[serde(default)]
    pub congestion: CongestionConfig,
    // Port hopping (QUIC only)
    #[serde(default)]
    pub port_hopping: crate::port_hop::PortHoppingConfig,
    // Salamander UDP obfuscation (QUIC only)
    #[serde(default)]
    pub salamander_password: Option<String>,
    // FEC (Forward Error Correction) for UDP relay
    #[serde(default)]
    pub udp_fec: crate::fec::FecConfig,
    // DNS handling
    #[serde(default)]
    pub dns: crate::dns::DnsConfig,
    // Rule-based routing
    #[serde(default)]
    pub routing: crate::router::RoutingConfig,
    // TUN mode
    #[serde(default)]
    pub tun: TunConfig,
}

/// TUN device configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_device_name")]
    pub device_name: String,
    #[serde(default = "default_mtu")]
    pub mtu: u16,
    /// Routes to capture (default: all traffic).
    #[serde(default = "default_include_routes")]
    pub include_routes: Vec<String>,
    /// Routes to exclude (e.g., the proxy server itself).
    #[serde(default)]
    pub exclude_routes: Vec<String>,
    /// DNS mode override for TUN mode.
    #[serde(default = "default_tun_dns")]
    pub dns: String,
}

impl Default for TunConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            device_name: default_device_name(),
            mtu: default_mtu(),
            include_routes: default_include_routes(),
            exclude_routes: Vec::new(),
            dns: default_tun_dns(),
        }
    }
}

fn default_device_name() -> String {
    "prisma-tun0".into()
}

fn default_mtu() -> u16 {
    1500
}

fn default_include_routes() -> Vec<String> {
    vec!["0.0.0.0/0".into()]
}

fn default_tun_dns() -> String {
    "fake".into()
}

/// Congestion control configuration for QUIC transport.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CongestionConfig {
    /// Mode: "brutal", "bbr", or "adaptive" (default: "bbr")
    #[serde(default = "default_congestion_mode")]
    pub mode: String,
    /// Target bandwidth for brutal/adaptive mode (e.g., "100mbps")
    #[serde(default)]
    pub target_bandwidth: Option<String>,
}

impl Default for CongestionConfig {
    fn default() -> Self {
        Self {
            mode: default_congestion_mode(),
            target_bandwidth: None,
        }
    }
}

fn default_congestion_mode() -> String {
    "bbr".into()
}

/// XMUX connection multiplexing configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XmuxConfig {
    #[serde(default = "default_xmux_max_connections_min")]
    pub max_connections_min: u16,
    #[serde(default = "default_xmux_max_connections_max")]
    pub max_connections_max: u16,
    #[serde(default = "default_xmux_max_concurrency_min")]
    pub max_concurrency_min: u16,
    #[serde(default = "default_xmux_max_concurrency_max")]
    pub max_concurrency_max: u16,
    #[serde(default = "default_xmux_max_lifetime_min")]
    pub max_lifetime_secs_min: u64,
    #[serde(default = "default_xmux_max_lifetime_max")]
    pub max_lifetime_secs_max: u64,
    #[serde(default = "default_xmux_max_requests_min")]
    pub max_requests_min: u32,
    #[serde(default = "default_xmux_max_requests_max")]
    pub max_requests_max: u32,
}

impl Default for XmuxConfig {
    fn default() -> Self {
        Self {
            max_connections_min: default_xmux_max_connections_min(),
            max_connections_max: default_xmux_max_connections_max(),
            max_concurrency_min: default_xmux_max_concurrency_min(),
            max_concurrency_max: default_xmux_max_concurrency_max(),
            max_lifetime_secs_min: default_xmux_max_lifetime_min(),
            max_lifetime_secs_max: default_xmux_max_lifetime_max(),
            max_requests_min: default_xmux_max_requests_min(),
            max_requests_max: default_xmux_max_requests_max(),
        }
    }
}

fn default_xmux_max_connections_min() -> u16 {
    1
}
fn default_xmux_max_connections_max() -> u16 {
    4
}
fn default_xmux_max_concurrency_min() -> u16 {
    8
}
fn default_xmux_max_concurrency_max() -> u16 {
    16
}
fn default_xmux_max_lifetime_min() -> u64 {
    300
}
fn default_xmux_max_lifetime_max() -> u64 {
    600
}
fn default_xmux_max_requests_min() -> u32 {
    100
}
fn default_xmux_max_requests_max() -> u32 {
    200
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
