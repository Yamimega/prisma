use tracing_subscriber::{fmt, EnvFilter};

/// Initialize the logging/tracing system.
///
/// `level` is one of: trace, debug, info, warn, error
/// `format` is one of: pretty, json
pub fn init_logging(level: &str, format: &str) {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level));

    match format {
        "json" => {
            fmt().with_env_filter(filter).json().init();
        }
        _ => {
            fmt().with_env_filter(filter).pretty().init();
        }
    }
}

/// Initialize logging with a broadcast layer for the management API.
///
/// Log entries are sent on `log_tx` for WebSocket subscribers.
/// The broadcast layer is additive — console output still works.
pub fn init_logging_with_broadcast(
    level: &str,
    format: &str,
    log_tx: tokio::sync::broadcast::Sender<BroadcastLogEntry>,
) {
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level));
    let broadcast_layer = BroadcastLayer { tx: log_tx };

    let registry = tracing_subscriber::registry()
        .with(filter)
        .with(broadcast_layer);

    match format {
        "json" => registry.with(fmt::layer().json()).init(),
        _ => registry.with(fmt::layer().pretty()).init(),
    }
}

/// A log entry broadcast over the channel.
#[derive(Debug, Clone, serde::Serialize)]
pub struct BroadcastLogEntry {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub level: String,
    pub target: String,
    pub message: String,
}

/// Tracing layer that broadcasts log events.
struct BroadcastLayer {
    tx: tokio::sync::broadcast::Sender<BroadcastLogEntry>,
}

impl<S> tracing_subscriber::Layer<S> for BroadcastLayer
where
    S: tracing::Subscriber,
{
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let mut visitor = MessageVisitor(String::new());
        event.record(&mut visitor);

        let entry = BroadcastLogEntry {
            timestamp: chrono::Utc::now(),
            level: event.metadata().level().to_string(),
            target: event.metadata().target().to_string(),
            message: visitor.0,
        };

        let _ = self.tx.send(entry);
    }
}

struct MessageVisitor(String);

impl tracing::field::Visit for MessageVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if !self.0.is_empty() {
            self.0.push(' ');
        }
        if field.name() == "message" {
            self.0.push_str(&format!("{:?}", value));
        } else {
            self.0.push_str(&format!("{}={:?}", field.name(), value));
        }
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if !self.0.is_empty() {
            self.0.push(' ');
        }
        if field.name() == "message" {
            self.0.push_str(value);
        } else {
            self.0.push_str(&format!("{}={}", field.name(), value));
        }
    }
}
