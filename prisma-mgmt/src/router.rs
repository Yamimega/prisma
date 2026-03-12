use std::sync::Arc;

use axum::extract::Request;
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::routing::{delete, get, post, put};
use axum::Router;
use tower_http::cors::{Any, CorsLayer};

use prisma_core::config::server::ManagementApiConfig;
use prisma_core::state::ServerState;

use crate::auth::{auth_middleware, AuthToken};
use crate::handlers::{clients, config, connections, forwards, health, routes};
use crate::ws::{logs, metrics};

pub fn build_router(config: ManagementApiConfig, state: ServerState) -> Router {
    let cors = if config.cors_origins.is_empty() {
        CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any)
    } else {
        let origins: Vec<_> = config
            .cors_origins
            .iter()
            .filter_map(|o| o.parse().ok())
            .collect();
        CorsLayer::new()
            .allow_origin(origins)
            .allow_methods(Any)
            .allow_headers(Any)
    };

    let auth_token = Arc::new(AuthToken(config.auth_token.clone()));

    let api = Router::new()
        // Health & metrics
        .route("/api/health", get(health::health))
        .route("/api/metrics", get(health::metrics))
        .route("/api/metrics/history", get(health::metrics_history))
        // Connections
        .route("/api/connections", get(connections::list))
        .route("/api/connections/{id}", delete(connections::disconnect))
        // Clients
        .route("/api/clients", get(clients::list))
        .route("/api/clients", post(clients::create))
        .route("/api/clients/{id}", put(clients::update))
        .route("/api/clients/{id}", delete(clients::remove))
        // Config
        .route("/api/config", get(config::get_config))
        .route("/api/config", axum::routing::patch(config::patch_config))
        .route("/api/config/tls", get(config::get_tls_info))
        // Forwards
        .route("/api/forwards", get(forwards::list))
        // Routes
        .route("/api/routes", get(routes::list))
        .route("/api/routes", post(routes::create))
        .route("/api/routes/{id}", put(routes::update))
        .route("/api/routes/{id}", delete(routes::remove))
        // WebSocket
        .route("/api/ws/metrics", get(metrics::ws_metrics))
        .route("/api/ws/logs", get(logs::ws_logs))
        // Auth middleware
        .layer(middleware::from_fn(auth_middleware))
        .layer(middleware::from_fn(move |mut req: Request, next: Next| {
            let token = auth_token.clone();
            async move {
                req.extensions_mut().insert((*token).clone());
                let resp: Response = next.run(req).await;
                Ok::<_, std::convert::Infallible>(resp)
            }
        }));

    api.layer(cors).with_state(state)
}
