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
    // XPorta transport (next-gen CDN transport)
    #[serde(default)]
    pub xporta: Option<XPortaClientConfig>,
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

/// XPorta client configuration — next-generation CDN transport.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XPortaClientConfig {
    /// Base URL of the CDN endpoint (e.g., "https://your-domain.com").
    pub base_url: String,
    /// Session initialization path.
    #[serde(default = "default_xporta_session_path")]
    pub session_path: String,
    /// Upload data paths (randomly chosen per request).
    #[serde(default = "default_xporta_data_paths")]
    pub data_paths: Vec<String>,
    /// Long-poll download paths (randomly chosen per request).
    #[serde(default = "default_xporta_poll_paths")]
    pub poll_paths: Vec<String>,
    /// Payload encoding: "json" (default, max stealth), "binary" (max throughput), "auto".
    #[serde(default = "default_xporta_encoding")]
    pub encoding: String,
    /// Number of concurrent pending poll requests.
    #[serde(default = "default_xporta_poll_concurrency")]
    pub poll_concurrency: u8,
    /// Max concurrent upload requests.
    #[serde(default = "default_xporta_upload_concurrency")]
    pub upload_concurrency: u8,
    /// Maximum payload size per request in bytes.
    #[serde(default = "default_xporta_max_payload_size")]
    pub max_payload_size: u32,
    /// Poll timeout in seconds (must be < 100 for Cloudflare).
    #[serde(default = "default_xporta_poll_timeout")]
    pub poll_timeout_secs: u16,
    /// Extra HTTP headers for all XPorta requests.
    #[serde(default)]
    pub extra_headers: Vec<(String, String)>,
    /// Session cookie name (must match server config).
    #[serde(default = "default_xporta_cookie_name")]
    pub cookie_name: String,
}

fn default_xporta_session_path() -> String {
    "/api/auth".into()
}
fn default_xporta_data_paths() -> Vec<String> {
    vec![
        "/api/v1/data".into(),
        "/api/v1/sync".into(),
        "/api/v1/update".into(),
    ]
}
fn default_xporta_poll_paths() -> Vec<String> {
    vec![
        "/api/v1/notifications".into(),
        "/api/v1/feed".into(),
        "/api/v1/events".into(),
    ]
}
fn default_xporta_encoding() -> String {
    "json".into()
}
fn default_xporta_poll_concurrency() -> u8 {
    3
}
fn default_xporta_upload_concurrency() -> u8 {
    4
}
fn default_xporta_max_payload_size() -> u32 {
    65536
}
fn default_xporta_poll_timeout() -> u16 {
    55
}
fn default_xporta_cookie_name() -> String {
    "_sess".into()
}
