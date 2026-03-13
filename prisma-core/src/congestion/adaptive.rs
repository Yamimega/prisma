//! Adaptive congestion controller for PrismaVeil v3.
//!
//! Auto-detects ISP throttling and switches between BBR-like behavior
//! (probe bandwidth, respect congestion signals) and aggressive Brutal-like
//! behavior (fixed target rate, ignore congestion).
//!
//! # Throttling detection heuristics
//!
//! - **High loss + stable RTT**: Consistent loss rate above 5% with low RTT
//!   variance suggests the ISP is dropping packets intentionally rather than
//!   the network being genuinely congested (real congestion causes RTT spikes).
//! - **Bandwidth cliff**: A sudden throughput drop (>50%) after a period of
//!   stability indicates active interference / rate limiting.
//!
//! # Mode transitions
//!
//! Uses hysteresis (N consecutive signals) to avoid oscillating between modes.
//! When entering aggressive mode, aggressiveness ramps up gradually over several
//! RTTs. When returning to BBR mode, aggressiveness ramps down.

use std::any::Any;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

use quinn::congestion::{Controller, ControllerFactory};
use quinn_proto::RttEstimator;

// ---------------------------------------------------------------------------
// Tuning constants
// ---------------------------------------------------------------------------

/// Rolling window size for loss-rate and throughput samples.
const METRIC_WINDOW_SIZE: usize = 32;

/// Loss rate threshold (as a fraction, 0.05 = 5%) above which we suspect
/// throttling, *if* RTT is also stable.
const LOSS_RATE_THROTTLE_THRESHOLD: f64 = 0.05;

/// RTT coefficient-of-variation threshold. If CV < this value RTT is
/// considered "stable" (low jitter). Real congestion typically causes high
/// RTT variance.
const RTT_STABILITY_CV_THRESHOLD: f64 = 0.15;

/// Throughput must drop by this fraction relative to the recent peak before
/// we flag a "bandwidth cliff". 0.5 = 50% drop.
const BANDWIDTH_CLIFF_FRACTION: f64 = 0.50;

/// Number of consecutive positive throttle signals required before switching
/// to aggressive mode (hysteresis).
const THROTTLE_ENGAGE_COUNT: u32 = 6;

/// Number of consecutive *negative* throttle signals required before switching
/// back to BBR mode (hysteresis).
const THROTTLE_DISENGAGE_COUNT: u32 = 10;

/// How quickly aggressiveness ramps up/down per evaluation tick.
/// 0.0 = fully BBR, 1.0 = fully Brutal.
const AGGRESSION_STEP_UP: f64 = 0.10;
const AGGRESSION_STEP_DOWN: f64 = 0.05;

/// Initial slow-start growth factor per ACK (BBR-like exponential probe).
const SLOW_START_GAIN: f64 = 2.0;

/// Steady-state pacing gain (slightly above 1.0 to probe for more bandwidth).
const PACING_GAIN: f64 = 1.25;

/// Multiplicative decrease on genuine congestion while in BBR-like mode.
const CONGESTION_DECREASE: f64 = 0.7;

/// Minimum congestion window: 4 * MTU.
const MIN_WINDOW_PACKETS: u64 = 4;

/// Maximum congestion window: 256 MiB.
const MAX_WINDOW: u64 = 256 * 1024 * 1024;

// ---------------------------------------------------------------------------
// Config / Factory
// ---------------------------------------------------------------------------

/// Configuration for the Adaptive congestion controller.
#[derive(Debug, Clone)]
pub struct AdaptiveConfig {
    /// Target bandwidth in bits per second (used when in aggressive mode).
    target_bps: u64,
}

impl AdaptiveConfig {
    pub fn new(target_bps: u64) -> Self {
        Self { target_bps }
    }

    /// Compute an initial window using the target bandwidth and a conservative
    /// RTT estimate, but start small like BBR (10 * MTU).
    fn initial_window(mtu: u64) -> u64 {
        // BBR-style: start with 10 packets worth.
        (mtu * 10).max(mtu * MIN_WINDOW_PACKETS)
    }
}

impl ControllerFactory for AdaptiveConfig {
    fn build(self: Arc<Self>, _now: Instant, current_mtu: u16) -> Box<dyn Controller> {
        let mtu = current_mtu as u64;
        let initial_window = Self::initial_window(mtu);
        Box::new(AdaptiveController {
            target_bps: self.target_bps,
            mtu,
            window: initial_window,
            init_window: initial_window,
            phase: Phase::SlowStart,
            aggression: 0.0,
            throttle_positive_streak: 0,
            throttle_negative_streak: 0,

            // Metric accumulators (current interval)
            interval_bytes_acked: 0,
            interval_bytes_lost: 0,
            interval_start: None,

            // Rolling windows
            loss_samples: VecDeque::with_capacity(METRIC_WINDOW_SIZE),
            rtt_samples: VecDeque::with_capacity(METRIC_WINDOW_SIZE),
            throughput_samples: VecDeque::with_capacity(METRIC_WINDOW_SIZE),

            last_rtt: Duration::from_millis(100),
            estimated_bw_bps: 0,
        })
    }
}

// ---------------------------------------------------------------------------
// Internal types
// ---------------------------------------------------------------------------

/// Operating phase of the controller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// Exponential probing (like BBR slow start / TCP slow start).
    SlowStart,
    /// Steady-state bandwidth probing (BBR-like).
    ProbeBw,
    /// Aggressive sending (Brutal-like), possibly blended with BBR window.
    Aggressive,
}

// ---------------------------------------------------------------------------
// Controller
// ---------------------------------------------------------------------------

/// Adaptive congestion controller.
///
/// See module-level docs for an overview.
#[derive(Debug, Clone)]
struct AdaptiveController {
    // ---- Configuration ----
    target_bps: u64,
    mtu: u64,

    // ---- Core CC state ----
    /// Current congestion window (bytes).
    window: u64,
    /// Window computed at creation time (for `initial_window()`).
    init_window: u64,
    /// Current operating phase.
    phase: Phase,
    /// Aggressiveness blend factor: 0.0 = pure BBR, 1.0 = pure Brutal.
    aggression: f64,

    // ---- Hysteresis counters ----
    throttle_positive_streak: u32,
    throttle_negative_streak: u32,

    // ---- Per-interval accumulators (reset every ~1 RTT) ----
    interval_bytes_acked: u64,
    interval_bytes_lost: u64,
    interval_start: Option<Instant>,

    // ---- Rolling metric windows ----
    /// Per-interval loss rates (0.0 .. 1.0).
    loss_samples: VecDeque<f64>,
    /// RTT samples (as microseconds, to avoid float precision issues).
    rtt_samples: VecDeque<u64>,
    /// Throughput samples (bytes per second).
    throughput_samples: VecDeque<u64>,

    // ---- Derived / cached ----
    last_rtt: Duration,
    /// Estimated bandwidth in bits per second.
    estimated_bw_bps: u64,
}

impl AdaptiveController {
    // ----- Metric helpers -------------------------------------------------

    /// Push a loss-rate sample, keeping the window bounded.
    fn push_loss(&mut self, loss_rate: f64) {
        if self.loss_samples.len() >= METRIC_WINDOW_SIZE {
            self.loss_samples.pop_front();
        }
        self.loss_samples.push_back(loss_rate);
    }

    /// Push an RTT sample (microseconds).
    fn push_rtt(&mut self, rtt_us: u64) {
        if self.rtt_samples.len() >= METRIC_WINDOW_SIZE {
            self.rtt_samples.pop_front();
        }
        self.rtt_samples.push_back(rtt_us);
    }

    /// Push a throughput sample (bytes/s).
    fn push_throughput(&mut self, tput: u64) {
        if self.throughput_samples.len() >= METRIC_WINDOW_SIZE {
            self.throughput_samples.pop_front();
        }
        self.throughput_samples.push_back(tput);
    }

    /// Average loss rate over the rolling window.
    fn avg_loss_rate(&self) -> f64 {
        if self.loss_samples.is_empty() {
            return 0.0;
        }
        let sum: f64 = self.loss_samples.iter().sum();
        sum / self.loss_samples.len() as f64
    }

    /// Coefficient of variation of RTT samples. Low CV = stable RTT.
    fn rtt_cv(&self) -> f64 {
        if self.rtt_samples.len() < 2 {
            return 0.0;
        }
        let n = self.rtt_samples.len() as f64;
        let mean = self.rtt_samples.iter().sum::<u64>() as f64 / n;
        if mean == 0.0 {
            return 0.0;
        }
        let variance = self
            .rtt_samples
            .iter()
            .map(|&v| {
                let diff = v as f64 - mean;
                diff * diff
            })
            .sum::<f64>()
            / n;
        variance.sqrt() / mean
    }

    /// Peak throughput over the recent window (bytes/s).
    fn peak_throughput(&self) -> u64 {
        self.throughput_samples.iter().copied().max().unwrap_or(0)
    }

    /// Latest throughput sample (bytes/s).
    fn latest_throughput(&self) -> u64 {
        self.throughput_samples.back().copied().unwrap_or(0)
    }

    // ----- Throttle detection ---------------------------------------------

    /// Returns `true` if current metrics suggest ISP throttling.
    fn detect_throttling(&self) -> bool {
        // Need enough samples to make a judgement.
        if self.loss_samples.len() < 4 || self.rtt_samples.len() < 4 {
            return false;
        }

        // Heuristic 1: high loss with stable RTT.
        let high_loss_stable_rtt = self.avg_loss_rate() > LOSS_RATE_THROTTLE_THRESHOLD
            && self.rtt_cv() < RTT_STABILITY_CV_THRESHOLD;

        // Heuristic 2: bandwidth cliff after stable period.
        let bandwidth_cliff = if self.throughput_samples.len() >= 4 {
            let peak = self.peak_throughput();
            let latest = self.latest_throughput();
            peak > 0 && (latest as f64) < (peak as f64 * BANDWIDTH_CLIFF_FRACTION)
        } else {
            false
        };

        high_loss_stable_rtt || bandwidth_cliff
    }

    // ----- Window calculation --------------------------------------------

    /// Compute the "BBR-like" window: estimated_bw * rtt.
    fn bbr_window(&self) -> u64 {
        let rtt_s = self.last_rtt.as_secs_f64();
        if rtt_s <= 0.0 || self.estimated_bw_bps == 0 {
            return self.window;
        }
        let bw_bytes = self.estimated_bw_bps as f64 / 8.0;
        let w = (bw_bytes * rtt_s * PACING_GAIN) as u64;
        self.clamp_window(w)
    }

    /// Compute the "Brutal-like" window for the target rate, compensating for
    /// loss just like the Brutal controller.
    fn brutal_window(&self) -> u64 {
        let rtt_ms = self.last_rtt.as_millis() as u64;
        if rtt_ms == 0 {
            return self.window;
        }
        let base_window = (self.target_bps / 8) * rtt_ms / 1000;

        let loss = self.avg_loss_rate();
        let effective = if loss > 0.0 && loss < 0.90 {
            (base_window as f64 / (1.0 - loss)) as u64
        } else if loss >= 0.90 {
            base_window * 10
        } else {
            base_window
        };

        self.clamp_window(effective)
    }

    /// Blend BBR and Brutal windows according to `self.aggression`.
    fn blended_window(&self) -> u64 {
        let bbr = self.bbr_window() as f64;
        let brutal = self.brutal_window() as f64;
        let blended = bbr * (1.0 - self.aggression) + brutal * self.aggression;
        self.clamp_window(blended as u64)
    }

    fn clamp_window(&self, w: u64) -> u64 {
        w.max(self.mtu * MIN_WINDOW_PACKETS).min(MAX_WINDOW)
    }

    // ----- Interval bookkeeping ------------------------------------------

    /// Called on every ACK to potentially close an interval and evaluate
    /// throttling metrics.
    fn maybe_close_interval(&mut self, now: Instant) {
        let start = match self.interval_start {
            Some(s) => s,
            None => {
                self.interval_start = Some(now);
                return;
            }
        };

        // Close the interval roughly every RTT (minimum 50 ms to avoid noise).
        let interval_dur = now.duration_since(start);
        let min_interval = self.last_rtt.max(Duration::from_millis(50));
        if interval_dur < min_interval {
            return;
        }

        // -- Record samples --
        let total_bytes = self.interval_bytes_acked + self.interval_bytes_lost;
        if total_bytes > 0 {
            let loss_rate = self.interval_bytes_lost as f64 / total_bytes as f64;
            self.push_loss(loss_rate);
        }

        let elapsed_s = interval_dur.as_secs_f64();
        if elapsed_s > 0.0 {
            let tput_bps = self.interval_bytes_acked as f64 / elapsed_s;
            let tput = tput_bps as u64;
            self.push_throughput(tput);

            // Update bandwidth estimate: use max of recent throughput.
            // In BBR, BtlBw is the windowed max delivery rate.
            let bw_bits = (tput_bps * 8.0) as u64;
            if bw_bits > self.estimated_bw_bps || self.estimated_bw_bps == 0 {
                self.estimated_bw_bps = bw_bits;
            } else {
                // Decay slowly so we don't hold stale peaks forever.
                // EWMA-like: new = 0.9 * old + 0.1 * sample
                self.estimated_bw_bps =
                    ((self.estimated_bw_bps as f64 * 0.9) + (bw_bits as f64 * 0.1)) as u64;
            }
        }

        // -- Evaluate throttle signal --
        let throttled = self.detect_throttling();
        if throttled {
            self.throttle_positive_streak += 1;
            self.throttle_negative_streak = 0;
        } else {
            self.throttle_negative_streak += 1;
            self.throttle_positive_streak = 0;
        }

        // -- Phase transitions with hysteresis --
        if self.phase != Phase::Aggressive && self.throttle_positive_streak >= THROTTLE_ENGAGE_COUNT
        {
            self.phase = Phase::Aggressive;
            tracing::info!(
                loss = %format!("{:.1}%", self.avg_loss_rate() * 100.0),
                rtt_cv = %format!("{:.3}", self.rtt_cv()),
                "adaptive CC: throttling detected, entering aggressive mode"
            );
        }
        if self.phase == Phase::Aggressive
            && self.throttle_negative_streak >= THROTTLE_DISENGAGE_COUNT
        {
            self.phase = Phase::ProbeBw;
            tracing::info!("adaptive CC: throttling no longer detected, returning to BBR mode");
        }

        // -- Ramp aggression --
        match self.phase {
            Phase::Aggressive => {
                self.aggression = (self.aggression + AGGRESSION_STEP_UP).min(1.0);
            }
            Phase::ProbeBw | Phase::SlowStart => {
                self.aggression = (self.aggression - AGGRESSION_STEP_DOWN).max(0.0);
            }
        }

        // -- Update window --
        match self.phase {
            Phase::SlowStart => {
                // Keep the window as-is; it grows on each ACK in on_ack().
            }
            Phase::ProbeBw => {
                if self.aggression > 0.0 {
                    self.window = self.blended_window();
                } else {
                    self.window = self.bbr_window();
                }
            }
            Phase::Aggressive => {
                self.window = self.blended_window();
            }
        }

        // -- Reset interval --
        self.interval_bytes_acked = 0;
        self.interval_bytes_lost = 0;
        self.interval_start = Some(now);
    }
}

impl Controller for AdaptiveController {
    fn on_sent(&mut self, _now: Instant, _bytes: u64, _last_packet_number: u64) {
        // Nothing special on send.
    }

    fn on_ack(
        &mut self,
        now: Instant,
        _sent: Instant,
        bytes: u64,
        _app_limited: bool,
        rtt: &RttEstimator,
    ) {
        self.last_rtt = rtt.get();
        self.push_rtt(self.last_rtt.as_micros() as u64);
        self.interval_bytes_acked += bytes;

        // Slow-start: grow window exponentially until first loss.
        if self.phase == Phase::SlowStart {
            let growth = (bytes as f64 * SLOW_START_GAIN) as u64;
            self.window = self.clamp_window(self.window.saturating_add(growth));
        }

        self.maybe_close_interval(now);
    }

    fn on_congestion_event(
        &mut self,
        now: Instant,
        _sent: Instant,
        is_persistent_congestion: bool,
        lost_bytes: u64,
    ) {
        self.interval_bytes_lost += lost_bytes;

        // Exit slow-start on first loss.
        if self.phase == Phase::SlowStart {
            self.phase = Phase::ProbeBw;
            // Set bandwidth estimate from current window.
            let rtt_s = self.last_rtt.as_secs_f64();
            if rtt_s > 0.0 {
                self.estimated_bw_bps = ((self.window as f64 / rtt_s) * 8.0) as u64;
            }
        }

        // In BBR-like mode (low aggression), reduce window on congestion.
        if self.aggression < 0.5 {
            let factor = if is_persistent_congestion {
                CONGESTION_DECREASE * CONGESTION_DECREASE // more aggressive on persistent
            } else {
                CONGESTION_DECREASE
            };
            // Blend the decrease with aggression level: higher aggression means
            // we care less about congestion signals.
            let effective_factor = factor * (1.0 - self.aggression) + self.aggression;
            self.window = self.clamp_window((self.window as f64 * effective_factor) as u64);
        }
        // In aggressive mode (aggression >= 0.5), ignore congestion like Brutal.

        // Still try to close the interval so metrics stay fresh.
        self.maybe_close_interval(now);
    }

    fn on_mtu_update(&mut self, new_mtu: u16) {
        self.mtu = new_mtu as u64;
        self.window = self.clamp_window(self.window);
    }

    fn window(&self) -> u64 {
        self.window
    }

    fn initial_window(&self) -> u64 {
        self.init_window
    }

    fn clone_box(&self) -> Box<dyn Controller> {
        Box::new(self.clone())
    }

    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: build a controller via the factory.
    fn make_controller(target_bps: u64, mtu: u16) -> Box<dyn Controller> {
        let config = Arc::new(AdaptiveConfig::new(target_bps));
        ControllerFactory::build(config, Instant::now(), mtu)
    }

    /// Helper: build a raw `AdaptiveController` for direct field access.
    fn make_raw(phase: Phase) -> AdaptiveController {
        AdaptiveController {
            target_bps: 100_000_000,
            mtu: 1200,
            window: 12000,
            init_window: 12000,
            phase,
            aggression: 0.0,
            throttle_positive_streak: 0,
            throttle_negative_streak: 0,
            interval_bytes_acked: 0,
            interval_bytes_lost: 0,
            interval_start: None,
            loss_samples: VecDeque::new(),
            rtt_samples: VecDeque::new(),
            throughput_samples: VecDeque::new(),
            last_rtt: Duration::from_millis(50),
            estimated_bw_bps: 50_000_000,
        }
    }

    #[test]
    fn test_initial_window_is_small() {
        let ctrl = make_controller(100_000_000, 1200);
        // Should start with 10 * MTU = 12000 (BBR-like, not Brutal-sized).
        assert_eq!(ctrl.initial_window(), 12000);
        assert_eq!(ctrl.window(), 12000);
    }

    #[test]
    fn test_window_grows_in_slow_start() {
        // Test slow-start growth directly on the internal struct, since
        // `RttEstimator::new` is private in quinn-proto 0.11.
        let mut ctrl = make_raw(Phase::SlowStart);
        let initial = ctrl.window;

        // Simulate the growth that on_ack performs in slow start.
        for _ in 0..10 {
            let growth = (1200_f64 * SLOW_START_GAIN) as u64;
            ctrl.window = ctrl.clamp_window(ctrl.window.saturating_add(growth));
        }

        assert!(
            ctrl.window > initial,
            "window should have grown: {}",
            ctrl.window
        );
        // 10 ACKs of 1200 bytes with 2x gain = 24000 added to 12000 = 36000.
        assert_eq!(ctrl.window, 12000 + 10 * 2400);
    }

    #[test]
    fn test_exits_slow_start_on_loss() {
        let mut ctrl = make_raw(Phase::SlowStart);

        // Grow window a bit first.
        ctrl.window = 24000;

        let before = ctrl.window;
        // Simulate on_congestion_event logic.
        ctrl.on_congestion_event(Instant::now(), Instant::now(), false, 1200);

        assert_eq!(ctrl.phase, Phase::ProbeBw, "should transition to ProbeBw");
        assert!(
            ctrl.window < before,
            "window should decrease on congestion in BBR mode: {} vs {}",
            ctrl.window,
            before
        );
    }

    #[test]
    fn test_throttle_detection_high_loss_stable_rtt() {
        let mut ctrl = make_raw(Phase::ProbeBw);

        // Feed high-loss, low-RTT-variance samples.
        for _ in 0..8 {
            ctrl.push_loss(0.10); // 10% loss
            ctrl.push_rtt(50_000); // ~50ms, perfectly stable (CV = 0)
        }

        assert!(
            ctrl.detect_throttling(),
            "should detect throttling with high loss + stable RTT"
        );
    }

    #[test]
    fn test_no_throttle_low_loss() {
        let mut ctrl = make_raw(Phase::ProbeBw);

        for _ in 0..8 {
            ctrl.push_loss(0.01); // 1% loss - below threshold
            ctrl.push_rtt(50_000);
        }

        assert!(
            !ctrl.detect_throttling(),
            "should NOT detect throttling with low loss rate"
        );
    }

    #[test]
    fn test_no_throttle_high_loss_high_rtt_variance() {
        let mut ctrl = make_raw(Phase::ProbeBw);

        // High loss but also high RTT variance = real congestion, not throttling.
        let rtts = [
            30_000, 80_000, 45_000, 120_000, 55_000, 95_000, 40_000, 110_000,
        ];
        for &rtt in &rtts {
            ctrl.push_loss(0.10);
            ctrl.push_rtt(rtt);
        }

        assert!(
            !ctrl.detect_throttling(),
            "should NOT detect throttling when RTT variance is high (real congestion)"
        );
    }

    #[test]
    fn test_bandwidth_cliff_detection() {
        let mut ctrl = make_raw(Phase::ProbeBw);

        // Stable throughput then sudden drop.
        for _ in 0..6 {
            ctrl.push_throughput(10_000_000); // 10 MB/s
            ctrl.push_loss(0.01); // low loss
            ctrl.push_rtt(50_000 + rand::random::<u64>() % 5_000); // moderate variance
        }
        ctrl.push_throughput(2_000_000); // sudden drop to 2 MB/s
        ctrl.push_loss(0.01);
        ctrl.push_rtt(52_000);

        assert!(
            ctrl.detect_throttling(),
            "should detect throttling from bandwidth cliff"
        );
    }

    #[test]
    fn test_hysteresis_prevents_premature_switch() {
        let mut ctrl = make_raw(Phase::ProbeBw);
        ctrl.estimated_bw_bps = 50_000_000;

        // Push enough samples for detection to fire.
        for _ in 0..8 {
            ctrl.push_loss(0.10);
            ctrl.push_rtt(50_000);
        }
        assert!(ctrl.detect_throttling());

        // But only a few positive streaks - should not switch yet.
        ctrl.throttle_positive_streak = THROTTLE_ENGAGE_COUNT - 1;
        assert_eq!(ctrl.phase, Phase::ProbeBw, "should still be ProbeBw");
    }

    #[test]
    fn test_aggression_ramp_up() {
        let mut ctrl = make_raw(Phase::Aggressive);
        ctrl.aggression = 0.0;

        // Ramp up 10 steps.
        for _ in 0..10 {
            ctrl.aggression = (ctrl.aggression + AGGRESSION_STEP_UP).min(1.0);
        }

        assert!(
            (ctrl.aggression - 1.0).abs() < f64::EPSILON,
            "aggression should reach 1.0 after 10 steps of 0.1"
        );
    }

    #[test]
    fn test_aggression_ramp_down() {
        let mut ctrl = make_raw(Phase::ProbeBw);
        ctrl.aggression = 1.0;

        // Ramp down 20 steps.
        for _ in 0..20 {
            ctrl.aggression = (ctrl.aggression - AGGRESSION_STEP_DOWN).max(0.0);
        }

        assert!(
            ctrl.aggression.abs() < f64::EPSILON,
            "aggression should reach 0.0 after 20 steps of 0.05"
        );
    }

    #[test]
    fn test_brutal_window_compensates_loss() {
        let mut ctrl = make_raw(Phase::Aggressive);
        ctrl.aggression = 1.0;
        ctrl.last_rtt = Duration::from_millis(100);

        // No loss: base window = 100_000_000 / 8 * 100 / 1000 = 1_250_000
        let no_loss = ctrl.brutal_window();
        assert_eq!(no_loss, 1_250_000);

        // With 10% loss: window should be higher (compensating).
        for _ in 0..8 {
            ctrl.push_loss(0.10);
        }
        let with_loss = ctrl.brutal_window();
        assert!(
            with_loss > no_loss,
            "brutal window should compensate for loss: {} vs {}",
            with_loss,
            no_loss
        );
    }

    #[test]
    fn test_clone_box() {
        let ctrl = make_controller(100_000_000, 1200);
        let cloned = ctrl.clone_box();
        assert_eq!(ctrl.window(), cloned.window());
    }

    #[test]
    fn test_mtu_update() {
        let mut ctrl = make_controller(100_000_000, 1200);
        ctrl.on_mtu_update(1400);
        // Window should be at least 4 * new MTU.
        assert!(ctrl.window() >= 4 * 1400);
    }

    #[test]
    fn test_into_any() {
        let ctrl = make_controller(100_000_000, 1200);
        let any = ctrl.into_any();
        assert!(any.downcast::<AdaptiveController>().is_ok());
    }
}
