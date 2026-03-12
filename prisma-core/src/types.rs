use serde::{Deserialize, Serialize};
use std::fmt;
use std::net::{Ipv4Addr, Ipv6Addr};
use uuid::Uuid;

// Protocol constants
pub const PROTOCOL_VERSION: u8 = 0x01;
pub const MAX_FRAME_SIZE: usize = 16384;
pub const NONCE_SIZE: usize = 12;
pub const MAX_PADDING_SIZE: usize = 256;

/// Unique client identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ClientId(pub Uuid);

impl ClientId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl Default for ClientId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for ClientId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Proxy destination address.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProxyAddress {
    Ipv4(Ipv4Addr),
    Ipv6(Ipv6Addr),
    Domain(String),
}

impl fmt::Display for ProxyAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProxyAddress::Ipv4(addr) => write!(f, "{}", addr),
            ProxyAddress::Ipv6(addr) => write!(f, "{}", addr),
            ProxyAddress::Domain(domain) => write!(f, "{}", domain),
        }
    }
}

/// Address type discriminator for wire format.
impl ProxyAddress {
    pub fn addr_type(&self) -> u8 {
        match self {
            ProxyAddress::Ipv4(_) => 0x01,
            ProxyAddress::Domain(_) => 0x03,
            ProxyAddress::Ipv6(_) => 0x04,
        }
    }
}

/// Proxy destination: address + port.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProxyDestination {
    pub address: ProxyAddress,
    pub port: u16,
}

impl fmt::Display for ProxyDestination {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.address {
            ProxyAddress::Ipv6(addr) => write!(f, "[{}]:{}", addr, self.port),
            _ => write!(f, "{}:{}", self.address, self.port),
        }
    }
}

/// Supported cipher suites.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum CipherSuite {
    #[default]
    ChaCha20Poly1305 = 0x01,
    Aes256Gcm = 0x02,
}

impl CipherSuite {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0x01 => Some(CipherSuite::ChaCha20Poly1305),
            0x02 => Some(CipherSuite::Aes256Gcm),
            _ => None,
        }
    }
}
