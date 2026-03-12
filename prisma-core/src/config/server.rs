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
}

impl Default for ManagementApiConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            listen_addr: default_mgmt_listen_addr(),
            auth_token: String::new(),
            cors_origins: Vec::new(),
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
}

impl Default for CamouflageConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            fallback_addr: None,
            tls_on_tcp: false,
            alpn_protocols: super::default_alpn(),
        }
    }
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
