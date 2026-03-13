use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::Serialize;
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

use crate::config::server::{AuthorizedClient, RoutingRule, ServerConfig};
use crate::util;

/// Shared client entry data for the auth store.
#[derive(Clone, Serialize)]
pub struct ClientEntry {
    #[serde(skip)]
    pub auth_secret: [u8; 32],
    pub name: Option<String>,
    pub enabled: bool,
}

/// The mutable inner data of AuthStore, shared between server and management API.
#[derive(Clone)]
pub struct AuthStoreInner {
    pub clients: HashMap<Uuid, ClientEntry>,
}

impl AuthStoreInner {
    pub fn from_config(clients: &[AuthorizedClient]) -> Result<Self, anyhow::Error> {
        let mut map = HashMap::new();
        for c in clients {
            let uuid = Uuid::parse_str(&c.id)
                .map_err(|e| anyhow::anyhow!("Invalid client UUID '{}': {}", c.id, e))?;
            let secret_bytes = util::hex_decode_32(&c.auth_secret)
                .map_err(|e| anyhow::anyhow!("Invalid auth_secret for '{}': {}", c.id, e))?;
            map.insert(
                uuid,
                ClientEntry {
                    auth_secret: secret_bytes,
                    name: c.name.clone(),
                    enabled: true,
                },
            );
        }
        Ok(Self { clients: map })
    }
}

/// Central server state shared across all tasks.
#[derive(Clone)]
pub struct ServerState {
    pub metrics: Arc<ServerMetrics>,
    pub connections: Arc<RwLock<HashMap<Uuid, ConnectionInfo>>>,
    pub auth_store: Arc<RwLock<AuthStoreInner>>,
    pub config: Arc<RwLock<ServerConfig>>,
    pub routing_rules: Arc<RwLock<Vec<RoutingRule>>>,
    pub log_tx: broadcast::Sender<LogEntry>,
    pub metrics_tx: broadcast::Sender<MetricsSnapshot>,
}

impl ServerState {
    pub fn new(
        config: &ServerConfig,
        auth_store: AuthStoreInner,
        log_tx: broadcast::Sender<LogEntry>,
        metrics_tx: broadcast::Sender<MetricsSnapshot>,
    ) -> Self {
        Self {
            metrics: Arc::new(ServerMetrics::new()),
            connections: Arc::new(RwLock::new(HashMap::new())),
            auth_store: Arc::new(RwLock::new(auth_store)),
            config: Arc::new(RwLock::new(config.clone())),
            routing_rules: Arc::new(RwLock::new(Vec::new())),
            log_tx,
            metrics_tx,
        }
    }

    pub fn snapshot_metrics(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            timestamp: Utc::now(),
            uptime_secs: self.metrics.started_at.elapsed().as_secs(),
            total_connections: self.metrics.total_connections.load(Ordering::Relaxed),
            active_connections: self.metrics.active_connections.load(Ordering::Relaxed),
            total_bytes_up: self.metrics.total_bytes_up.load(Ordering::Relaxed),
            total_bytes_down: self.metrics.total_bytes_down.load(Ordering::Relaxed),
            handshake_failures: self.metrics.handshake_failures.load(Ordering::Relaxed),
        }
    }
}

pub struct ServerMetrics {
    pub started_at: std::time::Instant,
    pub total_connections: AtomicU64,
    pub total_bytes_up: AtomicU64,
    pub total_bytes_down: AtomicU64,
    pub active_connections: AtomicUsize,
    pub handshake_failures: AtomicU64,
}

impl ServerMetrics {
    pub fn new() -> Self {
        Self {
            started_at: std::time::Instant::now(),
            total_connections: AtomicU64::new(0),
            total_bytes_up: AtomicU64::new(0),
            total_bytes_down: AtomicU64::new(0),
            active_connections: AtomicUsize::new(0),
            handshake_failures: AtomicU64::new(0),
        }
    }
}

impl Default for ServerMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Tracks a single active connection.
#[derive(Debug, Clone, Serialize)]
pub struct ConnectionInfo {
    pub session_id: Uuid,
    pub client_id: Option<Uuid>,
    pub client_name: Option<String>,
    pub peer_addr: String,
    pub transport: Transport,
    pub mode: SessionMode,
    pub connected_at: DateTime<Utc>,
    #[serde(skip)]
    pub bytes_up: Arc<AtomicU64>,
    #[serde(skip)]
    pub bytes_down: Arc<AtomicU64>,
}

impl ConnectionInfo {
    pub fn bytes_up_val(&self) -> u64 {
        self.bytes_up.load(Ordering::Relaxed)
    }

    pub fn bytes_down_val(&self) -> u64 {
        self.bytes_down.load(Ordering::Relaxed)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Transport {
    Tcp,
    Quic,
    WebSocket,
    Grpc,
    Xhttp,
    XPorta,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum SessionMode {
    Unknown,
    Proxy,
    Forward,
    UdpRelay,
}

#[derive(Debug, Clone, Serialize)]
pub struct MetricsSnapshot {
    pub timestamp: DateTime<Utc>,
    pub uptime_secs: u64,
    pub total_connections: u64,
    pub active_connections: usize,
    pub total_bytes_up: u64,
    pub total_bytes_down: u64,
    pub handshake_failures: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct LogEntry {
    pub timestamp: DateTime<Utc>,
    pub level: String,
    pub target: String,
    pub message: String,
}

/// Produces MetricsSnapshot every second on the broadcast channel.
pub async fn metrics_ticker(state: ServerState) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
    loop {
        interval.tick().await;
        // Skip snapshot when nobody is listening
        if state.metrics_tx.receiver_count() == 0 {
            continue;
        }
        let snapshot = state.snapshot_metrics();
        let _ = state.metrics_tx.send(snapshot);
    }
}
