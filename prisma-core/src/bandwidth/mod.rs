//! Bandwidth management: per-client rate limiting and traffic quotas.
//!
//! Uses token bucket rate limiting (via `governor`) for real-time bandwidth
//! control, and in-memory counters for traffic quota tracking.

pub mod limiter;
pub mod quota;
