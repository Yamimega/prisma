//! Congestion control implementations for QUIC transport.
//!
//! Provides three modes:
//! - **Brutal**: Fixed send rate, ignores congestion signals (Hysteria2-style)
//! - **BBR**: Google BBRv2 via quinn's built-in implementation
//! - **Adaptive**: Starts with BBR, switches to aggressive mode when throttling detected

pub mod adaptive;
pub mod brutal;

use std::sync::Arc;

use quinn::congestion::ControllerFactory;

/// Congestion control mode.
#[derive(Debug, Clone, Default)]
pub enum CongestionMode {
    /// Hysteria2-style brutal CC: send at target rate regardless of loss.
    Brutal {
        /// Target bandwidth in bits per second.
        target_bps: u64,
    },
    /// Google BBRv2 congestion control via quinn.
    #[default]
    Bbr,
    /// Adaptive CC: starts with BBR, switches to aggressive when throttling detected.
    Adaptive {
        /// Initial target bandwidth in bits per second (used when switching to aggressive mode).
        initial_bps: u64,
    },
}

// Default is derived via #[default] on Bbr variant

impl CongestionMode {
    /// Parse from config strings.
    pub fn from_config(mode: &str, target_bandwidth: Option<&str>) -> Self {
        let parse_bps = |s: &str| -> u64 {
            let s = s.trim().to_lowercase();
            if let Some(val) = s.strip_suffix("gbps") {
                val.trim().parse::<u64>().unwrap_or(100_000_000) * 1_000_000_000
            } else if let Some(val) = s.strip_suffix("mbps") {
                val.trim().parse::<u64>().unwrap_or(100) * 1_000_000
            } else if let Some(val) = s.strip_suffix("kbps") {
                val.trim().parse::<u64>().unwrap_or(100_000) * 1_000
            } else if let Some(val) = s.strip_suffix("bps") {
                val.trim().parse::<u64>().unwrap_or(100_000_000)
            } else {
                // Assume mbps by default
                s.parse::<u64>().unwrap_or(100) * 1_000_000
            }
        };

        match mode {
            "brutal" => {
                let bps = target_bandwidth.map(parse_bps).unwrap_or(100_000_000); // 100 Mbps default
                CongestionMode::Brutal { target_bps: bps }
            }
            "adaptive" => {
                let bps = target_bandwidth.map(parse_bps).unwrap_or(100_000_000);
                CongestionMode::Adaptive { initial_bps: bps }
            }
            _ => CongestionMode::Bbr,
        }
    }

    /// Create a quinn `ControllerFactory` from this mode.
    pub fn build_factory(&self) -> Arc<dyn ControllerFactory + Send + Sync> {
        match self {
            CongestionMode::Brutal { target_bps } => {
                Arc::new(brutal::BrutalConfig::new(*target_bps))
            }
            CongestionMode::Bbr => Arc::new(quinn::congestion::BbrConfig::default()),
            CongestionMode::Adaptive { initial_bps } => {
                Arc::new(adaptive::AdaptiveConfig::new(*initial_bps))
            }
        }
    }
}
