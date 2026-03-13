use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub listen_addr: String,
    pub quic_listen_addr: String,
    pub tls: Option<TlsConfig>,
    pub authorized_clients: Vec<AuthorizedClient>,
    #[serde(default)]
    pub logging: LoggingConfig,
    #[serde(default)]
    pub performance: PerformanceConfig,
    #[serde(default)]
    pub port_forwarding: PortForwardingConfig,
    #[serde(default)]
    pub management_api: ManagementApiConfig,
    #[serde(default)]
    pub camouflage: CamouflageConfig,
    #[serde(default)]
    pub cdn: CdnConfig,
    #[serde(default)]
    pub padding: PaddingConfig,
    // Congestion control (QUIC only)
    #[serde(default)]
    pub congestion: super::client::CongestionConfig,
    // Port hopping (QUIC only)
    #[serde(default)]
    pub port_hopping: crate::port_hop::PortHoppingConfig,
    /// Upstream DNS server for CMD_DNS_QUERY forwarding.
    #[serde(default = "default_dns_upstream")]
    pub dns_upstream: String,
}

fn default_dns_upstream() -> String {
    "8.8.8.8:53".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsConfig {
    pub cert_path: String,
    pub key_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorizedClient {
    pub id: String,
    pub auth_secret: String, // hex-encoded
    pub name: Option<String>,
    /// Per-client upload bandwidth limit (e.g., "100mbps").
    #[serde(default)]
    pub bandwidth_up: Option<String>,
    /// Per-client download bandwidth limit (e.g., "100mbps").
    #[serde(default)]
    pub bandwidth_down: Option<String>,
    /// Traffic quota (e.g., "100GB").
    #[serde(default)]
    pub quota: Option<String>,
    /// Quota reset period.
    #[serde(default)]
    pub quota_period: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    #[serde(default = "default_level")]
    pub level: String,
    #[serde(default = "default_format")]
    pub format: String,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: default_level(),
            format: default_format(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConfig {
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,
    #[serde(default = "default_timeout")]
    pub connection_timeout_secs: u64,
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            max_connections: default_max_connections(),
            connection_timeout_secs: default_timeout(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortForwardingConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_port_range_start")]
    pub port_range_start: u16,
    #[serde(default = "default_port_range_end")]
    pub port_range_end: u16,
}

impl PortForwardingConfig {
    pub fn is_port_allowed(&self, port: u16) -> bool {
        self.enabled && port >= self.port_range_start && port <= self.port_range_end
    }
}

impl Default for PortForwardingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            port_range_start: default_port_range_start(),
            port_range_end: default_port_range_end(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagementApiConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_mgmt_listen_addr")]
    pub listen_addr: String,
    #[serde(default)]
    pub auth_token: String,
    #[serde(default)]
    pub cors_origins: Vec<String>,
    #[serde(default)]
    pub dashboard_dir: Option<String>,
}

impl Default for ManagementApiConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            listen_addr: default_mgmt_listen_addr(),
            auth_token: String::new(),
            cors_origins: Vec::new(),
            dashboard_dir: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingRule {
    pub id: Uuid,
    pub name: String,
    pub priority: u32,
    pub condition: RuleCondition,
    pub action: RuleAction,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum RuleCondition {
    DomainMatch(String),
    DomainExact(String),
    IpCidr(String),
    PortRange(u16, u16),
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuleAction {
    Allow,
    Block,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CamouflageConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub fallback_addr: Option<String>,
    #[serde(default)]
    pub tls_on_tcp: bool,
    #[serde(default = "super::default_alpn")]
    pub alpn_protocols: Vec<String>,
    /// Salamander UDP obfuscation password. When set, QUIC packets are XOR-obfuscated.
    #[serde(default)]
    pub salamander_password: Option<String>,
    /// HTTP/3 masquerade: upstream URL to reverse-proxy for non-PrismaVeil QUIC connections.
    /// When set, active probers see a real website over HTTP/3.
    #[serde(default)]
    pub h3_cover_site: Option<String>,
    /// HTTP/3 masquerade: directory of static files to serve for non-PrismaVeil QUIC connections.
    /// Used as fallback when `h3_cover_site` is not set.
    #[serde(default)]
    pub h3_static_dir: Option<String>,
}

impl Default for CamouflageConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            fallback_addr: None,
            tls_on_tcp: false,
            alpn_protocols: super::default_alpn(),
            salamander_password: None,
            h3_cover_site: None,
            h3_static_dir: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CdnConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_cdn_listen_addr")]
    pub listen_addr: String,
    #[serde(default)]
    pub tls: Option<CdnTlsConfig>,
    #[serde(default = "default_ws_tunnel_path")]
    pub ws_tunnel_path: String,
    #[serde(default = "default_grpc_tunnel_path")]
    pub grpc_tunnel_path: String,
    #[serde(default)]
    pub cover_upstream: Option<String>,
    #[serde(default)]
    pub cover_static_dir: Option<String>,
    #[serde(default)]
    pub trusted_proxies: Vec<String>,
    #[serde(default)]
    pub expose_management_api: bool,
    #[serde(default = "default_management_api_path")]
    pub management_api_path: String,
    // XHTTP transport paths
    #[serde(default = "default_xhttp_upload_path")]
    pub xhttp_upload_path: String,
    #[serde(default = "default_xhttp_download_path")]
    pub xhttp_download_path: String,
    #[serde(default = "default_xhttp_stream_path")]
    pub xhttp_stream_path: String,
    #[serde(default)]
    pub xhttp_mode: Option<String>,
    #[serde(default)]
    pub xhttp_extra_headers: Vec<(String, String)>,
    #[serde(default)]
    pub xhttp_nosse: bool,
    // XPorta transport (next-gen CDN transport)
    #[serde(default)]
    pub xporta: Option<XPortaServerConfig>,
    // Header obfuscation
    #[serde(default)]
    pub response_server_header: Option<String>,
    #[serde(default = "default_true")]
    pub padding_header: bool,
    #[serde(default)]
    pub enable_sse_disguise: bool,
}

impl Default for CdnConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            listen_addr: default_cdn_listen_addr(),
            tls: None,
            ws_tunnel_path: default_ws_tunnel_path(),
            grpc_tunnel_path: default_grpc_tunnel_path(),
            cover_upstream: None,
            cover_static_dir: None,
            trusted_proxies: Vec::new(),
            expose_management_api: false,
            management_api_path: default_management_api_path(),
            xhttp_upload_path: default_xhttp_upload_path(),
            xhttp_download_path: default_xhttp_download_path(),
            xhttp_stream_path: default_xhttp_stream_path(),
            xhttp_mode: None,
            xhttp_extra_headers: Vec::new(),
            xhttp_nosse: false,
            xporta: None,
            response_server_header: None,
            padding_header: true,
            enable_sse_disguise: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CdnTlsConfig {
    pub cert_path: String,
    pub key_path: String,
}

/// Per-frame padding configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaddingConfig {
    #[serde(default = "default_padding_min")]
    pub min: u16,
    #[serde(default = "default_padding_max")]
    pub max: u16,
}

impl Default for PaddingConfig {
    fn default() -> Self {
        Self {
            min: default_padding_min(),
            max: default_padding_max(),
        }
    }
}

fn default_padding_min() -> u16 {
    0
}
fn default_padding_max() -> u16 {
    256
}

fn default_cdn_listen_addr() -> String {
    "0.0.0.0:443".into()
}
fn default_ws_tunnel_path() -> String {
    "/ws-tunnel".into()
}
fn default_grpc_tunnel_path() -> String {
    "/tunnel.PrismaTunnel".into()
}
fn default_management_api_path() -> String {
    "/prisma-mgmt".into()
}
fn default_xhttp_upload_path() -> String {
    "/api/v1/upload".into()
}
fn default_xhttp_download_path() -> String {
    "/api/v1/events".into()
}
fn default_xhttp_stream_path() -> String {
    "/api/v1/stream".into()
}
fn default_true() -> bool {
    true
}

fn default_mgmt_listen_addr() -> String {
    "127.0.0.1:9090".into()
}

fn default_port_range_start() -> u16 {
    1024
}
fn default_port_range_end() -> u16 {
    65535
}

/// XPorta server configuration — next-generation CDN transport.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XPortaServerConfig {
    /// Whether XPorta transport is enabled.
    #[serde(default)]
    pub enabled: bool,
    /// Session initialization path.
    #[serde(default = "default_xporta_session_path")]
    pub session_path: String,
    /// Upload data paths (must match client config).
    #[serde(default = "default_xporta_data_paths")]
    pub data_paths: Vec<String>,
    /// Long-poll download paths (must match client config).
    #[serde(default = "default_xporta_poll_paths")]
    pub poll_paths: Vec<String>,
    /// Session timeout in seconds.
    #[serde(default = "default_xporta_session_timeout")]
    pub session_timeout_secs: u64,
    /// Maximum concurrent sessions per client.
    #[serde(default = "default_xporta_max_sessions")]
    pub max_sessions_per_client: u16,
    /// Cookie name for session tokens.
    #[serde(default = "default_xporta_cookie_name")]
    pub cookie_name: String,
    /// Payload encoding: "json" (default) or "binary".
    #[serde(default = "default_xporta_encoding")]
    pub encoding: String,
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
fn default_xporta_session_timeout() -> u64 {
    300
}
fn default_xporta_max_sessions() -> u16 {
    8
}
fn default_xporta_cookie_name() -> String {
    "_sess".into()
}
fn default_xporta_encoding() -> String {
    "json".into()
}

fn default_level() -> String {
    "info".into()
}
fn default_format() -> String {
    "pretty".into()
}
fn default_max_connections() -> u32 {
    1024
}
fn default_timeout() -> u64 {
    300
}
