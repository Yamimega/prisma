//! Adaptive transport selector — auto-detects censorship and switches transport.
//!
//! Monitors connection health per transport and automatically falls back
//! to the next transport when the current one starts failing.
//!
//! Fallback order (configurable):
//! 1. QUIC v2 + Salamander (lowest latency)
//! 2. QUIC v2 plain
//! 3. TCP + PrismaTLS (best active probing resistance)
//! 4. WebSocket over CDN
//! 5. XPorta over CDN (last resort)

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{info, warn};

/// Transport types in fallback order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransportType {
    QuicV2Salamander,
    QuicV2,
    PrismaTls,
    WebSocket,
    XPorta,
    // Legacy transports
    Quic,
    TcpTls,
    Tcp,
}

impl TransportType {
    /// Parse from config string.
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "quic-v2-salamander" | "quic_v2_salamander" => Some(TransportType::QuicV2Salamander),
            "quic-v2" | "quic_v2" => Some(TransportType::QuicV2),
            "reality" | "prisma-tls" => Some(TransportType::PrismaTls),
            "ws" | "websocket" | "ws-cdn" => Some(TransportType::WebSocket),
            "xporta" => Some(TransportType::XPorta),
            "quic" => Some(TransportType::Quic),
            "tls" | "tcp-tls" | "tls-on-tcp" => Some(TransportType::TcpTls),
            "tcp" => Some(TransportType::Tcp),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            TransportType::QuicV2Salamander => "quic-v2-salamander",
            TransportType::QuicV2 => "quic-v2",
            TransportType::PrismaTls => "prisma-tls",
            TransportType::WebSocket => "websocket",
            TransportType::XPorta => "xporta",
            TransportType::Quic => "quic",
            TransportType::TcpTls => "tls-on-tcp",
            TransportType::Tcp => "tcp",
        }
    }
}

/// Default v4 fallback order.
pub const DEFAULT_FALLBACK_ORDER: &[TransportType] = &[
    TransportType::QuicV2Salamander,
    TransportType::QuicV2,
    TransportType::PrismaTls,
    TransportType::WebSocket,
    TransportType::XPorta,
];

/// Health metrics for a single transport.
#[derive(Debug, Clone)]
struct TransportHealth {
    /// Total connection attempts.
    total_attempts: u64,
    /// Successful connections in the monitoring window.
    recent_successes: u64,
    /// Failed connections in the monitoring window.
    recent_failures: u64,
    /// Start of the current monitoring window.
    window_start: Instant,
    /// Last successful connection time.
    last_success: Option<Instant>,
    /// Whether this transport is currently considered healthy.
    healthy: bool,
}

impl TransportHealth {
    fn new() -> Self {
        Self {
            total_attempts: 0,
            recent_successes: 0,
            recent_failures: 0,
            window_start: Instant::now(),
            last_success: None,
            healthy: true,
        }
    }

    /// Reset the monitoring window if it's expired.
    fn maybe_reset_window(&mut self, window_duration: Duration) {
        if self.window_start.elapsed() >= window_duration {
            self.recent_successes = 0;
            self.recent_failures = 0;
            self.window_start = Instant::now();
            // Re-enable transport for retry after window reset
            self.healthy = true;
        }
    }

    fn record_success(&mut self) {
        self.total_attempts += 1;
        self.recent_successes += 1;
        self.last_success = Some(Instant::now());
        self.healthy = true;
    }

    fn record_failure(&mut self) {
        self.total_attempts += 1;
        self.recent_failures += 1;
    }

    /// Failure rate in the current window.
    fn failure_rate(&self) -> f64 {
        let total = self.recent_successes + self.recent_failures;
        if total == 0 {
            return 0.0;
        }
        self.recent_failures as f64 / total as f64
    }
}

/// Adaptive transport selector with health monitoring.
pub struct TransportSelector {
    /// Ordered list of transports to try.
    fallback_order: Vec<TransportType>,
    /// Health metrics per transport.
    health: Arc<RwLock<HashMap<TransportType, TransportHealth>>>,
    /// Monitoring window duration.
    window_duration: Duration,
    /// Failure rate threshold for marking a transport as unhealthy.
    failure_threshold: f64,
}

impl TransportSelector {
    pub fn new(fallback_order: Vec<TransportType>) -> Self {
        let mut health = HashMap::new();
        for &transport in &fallback_order {
            health.insert(transport, TransportHealth::new());
        }

        Self {
            fallback_order,
            health: Arc::new(RwLock::new(health)),
            window_duration: Duration::from_secs(300), // 5 minute window
            failure_threshold: 0.5,                    // >50% failure → unhealthy
        }
    }

    /// Parse fallback order from config strings.
    pub fn from_config(order: &[String]) -> Self {
        let transports: Vec<TransportType> = order
            .iter()
            .filter_map(|s| TransportType::from_str(s))
            .collect();

        if transports.is_empty() {
            Self::new(DEFAULT_FALLBACK_ORDER.to_vec())
        } else {
            Self::new(transports)
        }
    }

    /// Select the best available transport.
    ///
    /// Returns the first healthy transport in the fallback order.
    /// If all transports are unhealthy, returns the first one (retry from top).
    pub async fn select(&self) -> TransportType {
        let mut health = self.health.write().await;

        // Reset expired windows and check health
        for (transport, metrics) in health.iter_mut() {
            metrics.maybe_reset_window(self.window_duration);
            if metrics.failure_rate() > self.failure_threshold
                && metrics.recent_successes + metrics.recent_failures >= 3
            {
                if metrics.healthy {
                    warn!(
                        transport = %transport.as_str(),
                        failure_rate = %format!("{:.0}%", metrics.failure_rate() * 100.0),
                        "Transport marked unhealthy, will try fallback"
                    );
                }
                metrics.healthy = false;
            }
        }

        // Find first healthy transport
        for &transport in &self.fallback_order {
            if let Some(metrics) = health.get(&transport) {
                if metrics.healthy {
                    return transport;
                }
            }
        }

        // All unhealthy — reset all and try from top
        info!("All transports unhealthy, resetting health metrics");
        for metrics in health.values_mut() {
            metrics.recent_successes = 0;
            metrics.recent_failures = 0;
            metrics.healthy = true;
            metrics.window_start = Instant::now();
        }

        self.fallback_order[0]
    }

    /// Record a successful connection for a transport.
    pub async fn record_success(&self, transport: TransportType) {
        let mut health = self.health.write().await;
        if let Some(metrics) = health.get_mut(&transport) {
            metrics.record_success();
        }
    }

    /// Record a failed connection for a transport.
    pub async fn record_failure(&self, transport: TransportType) {
        let mut health = self.health.write().await;
        if let Some(metrics) = health.get_mut(&transport) {
            metrics.record_failure();
        }
    }

    /// Get a snapshot of transport health for monitoring/dashboard.
    pub async fn health_snapshot(&self) -> Vec<(TransportType, bool, f64)> {
        let health = self.health.read().await;
        self.fallback_order
            .iter()
            .filter_map(|&t| health.get(&t).map(|m| (t, m.healthy, m.failure_rate())))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_select_first_healthy() {
        let selector = TransportSelector::new(vec![
            TransportType::QuicV2,
            TransportType::PrismaTls,
            TransportType::WebSocket,
        ]);

        let selected = selector.select().await;
        assert_eq!(selected, TransportType::QuicV2);
    }

    #[tokio::test]
    async fn test_fallback_on_failure() {
        let selector =
            TransportSelector::new(vec![TransportType::QuicV2, TransportType::PrismaTls]);

        // Simulate QuicV2 failures
        for _ in 0..5 {
            selector.record_failure(TransportType::QuicV2).await;
        }

        let selected = selector.select().await;
        assert_eq!(selected, TransportType::PrismaTls);
    }

    #[tokio::test]
    async fn test_health_recovery() {
        let selector =
            TransportSelector::new(vec![TransportType::QuicV2, TransportType::PrismaTls]);

        // Fail QuicV2
        for _ in 0..5 {
            selector.record_failure(TransportType::QuicV2).await;
        }

        // Then succeed — should recover
        for _ in 0..5 {
            selector.record_success(TransportType::QuicV2).await;
        }

        let snapshot = selector.health_snapshot().await;
        let quic_health = snapshot
            .iter()
            .find(|(t, _, _)| *t == TransportType::QuicV2);
        assert!(quic_health.unwrap().1); // healthy = true
    }

    #[test]
    fn test_transport_type_parse() {
        assert_eq!(
            TransportType::from_str("quic-v2"),
            Some(TransportType::QuicV2)
        );
        assert_eq!(
            TransportType::from_str("reality"),
            Some(TransportType::PrismaTls)
        );
        assert_eq!(
            TransportType::from_str("prisma-tls"),
            Some(TransportType::PrismaTls)
        );
        assert_eq!(
            TransportType::from_str("websocket"),
            Some(TransportType::WebSocket)
        );
        assert_eq!(TransportType::from_str("invalid"), None);
    }
}
