use std::sync::atomic::Ordering;

use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status, Streaming};
use tracing::{info, warn};

use prisma_core::cache::DnsCache;
use prisma_core::config::server::ServerConfig;
use prisma_core::proto::tunnel::prisma_tunnel_server::PrismaTunnel;
use prisma_core::proto::tunnel::TunnelData;

use crate::auth::AuthStore;
use crate::grpc_stream::GrpcStream;
use crate::handler;
use crate::state::ServerContext;

#[derive(Clone)]
pub struct TunnelServiceImpl {
    pub config: ServerConfig,
    pub auth: AuthStore,
    pub dns: DnsCache,
    pub ctx: ServerContext,
}

#[tonic::async_trait]
impl PrismaTunnel for TunnelServiceImpl {
    type TunnelStream = ReceiverStream<Result<TunnelData, Status>>;

    async fn tunnel(
        &self,
        request: Request<Streaming<TunnelData>>,
    ) -> Result<Response<Self::TunnelStream>, Status> {
        let peer_ip = request
            .remote_addr()
            .map(|a| a.to_string())
            .unwrap_or_else(|| "unknown".into());

        info!(peer = %peer_ip, "gRPC tunnel connection");

        self.ctx
            .state
            .metrics
            .total_connections
            .fetch_add(1, Ordering::Relaxed);
        self.ctx
            .state
            .metrics
            .active_connections
            .fetch_add(1, Ordering::Relaxed);

        let inbound = request.into_inner();
        let (response_tx, response_rx) = mpsc::channel::<Result<TunnelData, Status>>(256);

        let grpc_stream = GrpcStream::new(inbound, response_tx);

        let config = self.config.clone();
        let auth = self.auth.clone();
        let dns = self.dns.clone();
        let ctx = self.ctx.clone();

        tokio::spawn(async move {
            let fwd = config.port_forwarding.clone();
            let result = handler::handle_tcp_connection_camouflaged(
                grpc_stream,
                auth,
                dns,
                fwd,
                ctx.clone(),
                peer_ip.clone(),
                None,
            )
            .await;

            if let Err(e) = result {
                warn!(peer = %peer_ip, error = %e, "gRPC tunnel error");
            }

            ctx.state
                .metrics
                .active_connections
                .fetch_sub(1, Ordering::Relaxed);
        });

        Ok(Response::new(ReceiverStream::new(response_rx)))
    }
}
