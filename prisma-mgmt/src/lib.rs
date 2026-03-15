mod auth;
mod handlers;
pub mod router;
mod ws;

use std::ops::Deref;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use prisma_core::bandwidth::{limiter::BandwidthLimiterStore, quota::QuotaStore};
use prisma_core::config::server::ManagementApiConfig;
use prisma_core::state::ServerState;
use tokio::sync::RwLock;
use tracing::info;

/// Alert thresholds configuration.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct AlertConfig {
    pub cert_expiry_days: u32,
    pub quota_warn_percent: u8,
    pub handshake_spike_threshold: u64,
}

impl Default for AlertConfig {
    fn default() -> Self {
        Self {
            cert_expiry_days: 30,
            quota_warn_percent: 80,
            handshake_spike_threshold: 100,
        }
    }
}

/// Extended management API state that wraps ServerState with additional stores.
#[derive(Clone)]
pub struct MgmtState {
    pub state: ServerState,
    pub bandwidth: Option<Arc<BandwidthLimiterStore>>,
    pub quotas: Option<Arc<QuotaStore>>,
    pub config_path: Option<PathBuf>,
    pub alert_config: Arc<RwLock<AlertConfig>>,
}

impl Deref for MgmtState {
    type Target = ServerState;
    fn deref(&self) -> &Self::Target {
        &self.state
    }
}

/// Start the management API server (HTTPS when TLS is configured, HTTP otherwise).
pub async fn serve(config: ManagementApiConfig, state: MgmtState) -> Result<()> {
    let app = router::build_router(config.clone(), state);

    if let Some(ref tls) = config.tls {
        let rustls_config =
            axum_server::tls_rustls::RustlsConfig::from_pem_file(&tls.cert_path, &tls.key_path)
                .await?;
        let addr: std::net::SocketAddr = config.listen_addr.parse()?;
        info!(addr = %config.listen_addr, "Management API started (HTTPS)");
        axum_server::bind_rustls(addr, rustls_config)
            .serve(app.into_make_service())
            .await?;
    } else {
        let listener = tokio::net::TcpListener::bind(&config.listen_addr).await?;
        info!(addr = %config.listen_addr, "Management API started (HTTP)");
        axum::serve(listener, app).await?;
    }

    Ok(())
}
