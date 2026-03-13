use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Result;
use axum::extract::Request;
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::routing::{any, get, post};
use axum::Router;
use axum_server::tls_rustls::RustlsConfig;
use rand::Rng;
use tower_http::services::{ServeDir, ServeFile};
use tracing::info;

use crate::auth::AuthStore;
use crate::listener::grpc_tunnel::TunnelServiceImpl;
use crate::listener::reverse_proxy::{self, ProxyState};
use crate::listener::ws_tunnel::{self, CdnState};
use crate::listener::xhttp;
use crate::state::ServerContext;
use prisma_core::cache::DnsCache;
use prisma_core::config::server::ServerConfig;
use prisma_core::proto::tunnel::prisma_tunnel_server::PrismaTunnelServer;

pub async fn listen(
    config: &ServerConfig,
    auth: AuthStore,
    dns: DnsCache,
    ctx: ServerContext,
) -> Result<()> {
    let state = ctx.state.clone();
    let cdn = &config.cdn;
    let addr: SocketAddr = cdn.listen_addr.parse()?;

    let tls_cfg = cdn
        .tls
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("CDN requires TLS configuration"))?;

    let rustls_config = RustlsConfig::from_pem_file(&tls_cfg.cert_path, &tls_cfg.key_path).await?;

    let cdn_state = CdnState {
        config: config.clone(),
        auth: auth.clone(),
        dns: dns.clone(),
        ctx,
        trusted_proxies: cdn.trusted_proxies.clone(),
    };

    let app = build_cdn_router(config, cdn_state, state)?;

    info!(addr = %addr, "CDN HTTPS listener started");

    axum_server::bind_rustls(addr, rustls_config)
        .serve(app.into_make_service_with_connect_info::<SocketAddr>())
        .await?;

    Ok(())
}

fn build_cdn_router(
    config: &ServerConfig,
    cdn_state: CdnState,
    state: prisma_core::state::ServerState,
) -> Result<Router> {
    let cdn = &config.cdn;

    // 1. WebSocket tunnel
    let mut app = Router::new()
        .route(&cdn.ws_tunnel_path, get(ws_tunnel::ws_tunnel_handler))
        .with_state(cdn_state.clone());

    // 2. gRPC tunnel service — mounted at the gRPC path using route_service
    let grpc_svc = TunnelServiceImpl {
        config: config.clone(),
        auth: cdn_state.auth.clone(),
        dns: cdn_state.dns.clone(),
        ctx: cdn_state.ctx.clone(),
    };
    let grpc_path = format!("{}/Tunnel", cdn.grpc_tunnel_path);
    app = app.route_service(&grpc_path, PrismaTunnelServer::new(grpc_svc));

    // 3. XHTTP transport routes
    let xhttp_state = xhttp::XhttpState {
        cdn: cdn_state.clone(),
        sessions: Arc::new(dashmap::DashMap::new()),
    };
    let xhttp_router = Router::new()
        .route(&cdn.xhttp_upload_path, post(xhttp::packet_upload_handler))
        .route(
            &cdn.xhttp_download_path,
            get(xhttp::packet_download_handler),
        )
        .route(&cdn.xhttp_stream_path, post(xhttp::stream_handler))
        .with_state(xhttp_state);
    app = app.merge(xhttp_router);

    // 4. Management API + dashboard on subpath (optional)
    if cdn.expose_management_api {
        let mgmt = prisma_mgmt::router::build_router(config.management_api.clone(), state);
        app = app.nest(&cdn.management_api_path, mgmt);
    }

    // 5. Cover traffic (fallback — lowest priority)
    if let Some(ref upstream) = cdn.cover_upstream {
        let proxy_state = ProxyState {
            upstream: upstream.clone(),
        };
        app = app.fallback(any(reverse_proxy::reverse_proxy).with_state(proxy_state));
    } else if let Some(ref dir) = cdn.cover_static_dir {
        let index_path = std::path::PathBuf::from(dir).join("index.html");
        let serve_dir = ServeDir::new(dir)
            .append_index_html_on_directories(true)
            .fallback(ServeFile::new(&index_path));
        app = app.fallback_service(serve_dir);
    }

    // 6. Header obfuscation middleware
    let server_header = cdn.response_server_header.clone();
    let add_padding_header = cdn.padding_header;
    let extra_headers = cdn.xhttp_extra_headers.clone();
    app = app.layer(middleware::from_fn(move |req: Request, next: Next| {
        let server_header = server_header.clone();
        let extra_headers = extra_headers.clone();
        async move {
            let mut response: Response = next.run(req).await;
            let headers = response.headers_mut();

            // Override Server header
            if let Some(ref server_val) = server_header {
                headers.insert(
                    "server",
                    server_val
                        .parse()
                        .unwrap_or_else(|_| "nginx".parse().unwrap()),
                );
            }

            // Add X-Padding response header with random-length value
            if add_padding_header {
                let mut rng = rand::thread_rng();
                let padding_len = rng.gen_range(16..=128);
                let padding: String = (0..padding_len)
                    .map(|_| rng.gen_range(b'a'..=b'z') as char)
                    .collect();
                if let Ok(val) = padding.parse() {
                    headers.insert("x-padding", val);
                }
            }

            // Add extra response headers
            for (k, v) in &extra_headers {
                if let (Ok(name), Ok(val)) = (k.parse::<axum::http::HeaderName>(), v.parse()) {
                    headers.insert(name, val);
                }
            }

            response
        }
    }));

    Ok(app)
}
