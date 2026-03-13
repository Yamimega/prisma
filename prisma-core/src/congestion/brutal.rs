//! Brutal congestion controller (Hysteria2-style).
//!
//! Sends at a fixed target rate regardless of packet loss or congestion signals.
//! This is effective against ISP throttling where the network intentionally drops
//! packets to slow down traffic.
//!
//! Formula: `send_rate = target_bps / (1 - loss_rate)`
//! Window:  `cwnd = target_bps * rtt / 8` (bits to bytes)

use std::any::Any;
use std::sync::Arc;
use std::time::{Duration, Instant};

use quinn::congestion::{Controller, ControllerFactory};
use quinn_proto::RttEstimator;

/// Configuration for the Brutal congestion controller.
#[derive(Debug, Clone)]
pub struct BrutalConfig {
    /// Target bandwidth in bits per second.
    target_bps: u64,
}

impl BrutalConfig {
    pub fn new(target_bps: u64) -> Self {
        Self { target_bps }
    }

    /// Calculate initial window from target bandwidth with a default RTT estimate.
    fn initial_window_for_bps(target_bps: u64, mtu: u64) -> u64 {
        // Assume 100ms RTT for initial window calculation
        let initial_rtt_ms: u64 = 100;
        let window = (target_bps / 8) * initial_rtt_ms / 1000;
        // Clamp to reasonable bounds
        window.max(mtu * 4).min(256 * 1024 * 1024) // Min 4 MTU, max 256 MB
    }
}

impl ControllerFactory for BrutalConfig {
    fn build(self: Arc<Self>, _now: Instant, current_mtu: u16) -> Box<dyn Controller> {
        Box::new(BrutalController {
            target_bps: self.target_bps,
            mtu: current_mtu as u64,
            window: Self::initial_window_for_bps(self.target_bps, current_mtu as u64),
            bytes_acked: 0,
            bytes_lost: 0,
            last_rtt: Duration::from_millis(100),
        })
    }
}

/// Brutal congestion controller that maintains a fixed send rate.
#[derive(Debug, Clone)]
struct BrutalController {
    /// Target bandwidth in bits per second.
    target_bps: u64,
    /// Current MTU.
    mtu: u64,
    /// Current congestion window (bytes).
    window: u64,
    /// Total bytes acknowledged (for loss rate estimation).
    bytes_acked: u64,
    /// Total bytes lost (for loss rate estimation).
    bytes_lost: u64,
    /// Last measured RTT.
    last_rtt: Duration,
}

impl BrutalController {
    /// Recalculate the congestion window based on target rate and current RTT.
    fn update_window(&mut self) {
        let rtt_ms = self.last_rtt.as_millis() as u64;
        if rtt_ms == 0 {
            return;
        }

        // Base window: target_bps * rtt / 8 (bits to bytes)
        let base_window = (self.target_bps / 8) * rtt_ms / 1000;

        // Compensate for loss: effective_rate = target / (1 - loss_rate)
        let total = self.bytes_acked + self.bytes_lost;
        let effective_window = if total > 0 && self.bytes_lost > 0 {
            let loss_rate_pct = (self.bytes_lost * 100) / total;
            if loss_rate_pct >= 90 {
                // Cap compensation at 10x (90% loss)
                base_window * 10
            } else {
                base_window * 100 / (100 - loss_rate_pct)
            }
        } else {
            base_window
        };

        // Clamp to reasonable bounds
        self.window = effective_window.max(self.mtu * 4).min(256 * 1024 * 1024);
    }
}

impl Controller for BrutalController {
    fn on_sent(&mut self, _now: Instant, _bytes: u64, _last_packet_number: u64) {
        // Brutal CC doesn't adjust on send
    }

    fn on_ack(
        &mut self,
        _now: Instant,
        _sent: Instant,
        bytes: u64,
        _app_limited: bool,
        rtt: &RttEstimator,
    ) {
        self.bytes_acked += bytes;
        self.last_rtt = rtt.get();
        self.update_window();
    }

    fn on_congestion_event(
        &mut self,
        _now: Instant,
        _sent: Instant,
        _is_persistent_congestion: bool,
        lost_bytes: u64,
    ) {
        // Brutal CC intentionally ignores congestion signals.
        // Only track loss for rate compensation.
        self.bytes_lost += lost_bytes;
        self.update_window();
    }

    fn on_mtu_update(&mut self, new_mtu: u16) {
        self.mtu = new_mtu as u64;
        self.update_window();
    }

    fn window(&self) -> u64 {
        self.window
    }

    fn clone_box(&self) -> Box<dyn Controller> {
        Box::new(self.clone())
    }

    fn initial_window(&self) -> u64 {
        self.window
    }

    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_brutal_initial_window() {
        let config = BrutalConfig::new(100_000_000); // 100 Mbps
        let controller = ControllerFactory::build(Arc::new(config), Instant::now(), 1200);

        // 100 Mbps = 12.5 MB/s, at 100ms RTT = 1.25 MB window
        let window = controller.window();
        assert!(window >= 1_000_000, "Window too small: {}", window);
        assert!(window <= 2_000_000, "Window too large: {}", window);
    }

    #[test]
    fn test_brutal_minimum_window() {
        let config = BrutalConfig::new(1000); // Very low rate
        let controller = ControllerFactory::build(Arc::new(config), Instant::now(), 1200);

        // Should be at least 4 MTU
        assert!(controller.window() >= 4800);
    }

    #[test]
    fn test_brutal_ignores_congestion() {
        let config = BrutalConfig::new(100_000_000);
        let mut controller = ControllerFactory::build(Arc::new(config), Instant::now(), 1200);

        let initial = controller.window();
        controller.on_congestion_event(Instant::now(), Instant::now(), false, 10000);

        // Window should not decrease from congestion (may increase due to loss compensation)
        assert!(controller.window() >= initial);
    }
}
