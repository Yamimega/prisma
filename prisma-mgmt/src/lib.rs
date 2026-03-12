mod auth;
mod handlers;
mod router;
mod ws;

use anyhow::Result;
use prisma_core::config::server::ManagementApiConfig;
use prisma_core::state::ServerState;
use tracing::info;

/// Start the management API HTTP server.
pub async fn serve(config: ManagementApiConfig, state: ServerState) -> Result<()> {
    let app = router::build_router(config.clone(), state);

    let listener = tokio::net::TcpListener::bind(&config.listen_addr).await?;
    info!(addr = %config.listen_addr, "Management API started");

    axum::serve(listener, app).await?;
    Ok(())
}
