/// Hand-written proto types for the tunnel service.

#[derive(Clone, PartialEq, prost::Message)]
pub struct TunnelData {
    #[prost(bytes = "vec", tag = "1")]
    pub payload: Vec<u8>,
}

/// Server-side types for the PrismaTunnel gRPC service.
pub mod prisma_tunnel_server {
    use super::TunnelData;
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::Arc;
    use tonic::{Request, Response, Status, Streaming};

    #[tonic::async_trait]
    pub trait PrismaTunnel: Send + Sync + 'static {
        type TunnelStream: tonic::codegen::tokio_stream::Stream<Item = Result<TunnelData, Status>>
            + Send
            + 'static;

        async fn tunnel(
            &self,
            request: Request<Streaming<TunnelData>>,
        ) -> Result<Response<Self::TunnelStream>, Status>;
    }

    /// Helper struct that implements StreamingService for the Tunnel method.
    struct TunnelSvc<T: PrismaTunnel>(Arc<T>);

    impl<T: PrismaTunnel> tonic::server::StreamingService<TunnelData> for TunnelSvc<T> {
        type Response = TunnelData;
        type ResponseStream = T::TunnelStream;
        type Future =
            Pin<Box<dyn Future<Output = Result<Response<Self::ResponseStream>, Status>> + Send>>;

        fn call(&mut self, request: Request<Streaming<TunnelData>>) -> Self::Future {
            let inner = self.0.clone();
            Box::pin(async move { inner.tunnel(request).await })
        }
    }

    #[derive(Debug)]
    pub struct PrismaTunnelServer<T: PrismaTunnel> {
        inner: Arc<T>,
    }

    impl<T: PrismaTunnel> PrismaTunnelServer<T> {
        pub fn new(inner: T) -> Self {
            Self {
                inner: Arc::new(inner),
            }
        }
    }

    impl<T: PrismaTunnel> Clone for PrismaTunnelServer<T> {
        fn clone(&self) -> Self {
            Self {
                inner: self.inner.clone(),
            }
        }
    }

    impl<T, B> tonic::codegen::Service<http::Request<B>> for PrismaTunnelServer<T>
    where
        T: PrismaTunnel,
        B: http_body::Body + Send + 'static,
        B::Error: Into<Box<dyn std::error::Error + Send + Sync>> + Send + 'static,
    {
        type Response = http::Response<tonic::body::Body>;
        type Error = std::convert::Infallible;
        type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

        fn poll_ready(
            &mut self,
            _cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Result<(), Self::Error>> {
            std::task::Poll::Ready(Ok(()))
        }

        fn call(&mut self, req: http::Request<B>) -> Self::Future {
            let inner = self.inner.clone();

            // Match on the method path
            let path = req.uri().path().to_string();
            if path.ends_with("/Tunnel") {
                let fut = async move {
                    let codec = tonic::codec::ProstCodec::<TunnelData, TunnelData>::default();
                    let mut grpc = tonic::server::Grpc::new(codec);
                    let svc = TunnelSvc(inner);
                    let res = grpc.streaming(svc, req).await;
                    Ok(res)
                };
                Box::pin(fut)
            } else {
                Box::pin(async move {
                    let resp = http::Response::builder()
                        .status(http::StatusCode::NOT_FOUND)
                        .body(tonic::body::Body::default())
                        .unwrap();
                    Ok(resp)
                })
            }
        }
    }

    impl<T: PrismaTunnel> tonic::server::NamedService for PrismaTunnelServer<T> {
        const NAME: &'static str = "tunnel.PrismaTunnel";
    }
}

/// Client-side types for the PrismaTunnel gRPC service.
pub mod prisma_tunnel_client {
    use super::TunnelData;
    use tonic::{Response, Status, Streaming};

    #[derive(Debug, Clone)]
    pub struct PrismaTunnelClient<T> {
        inner: tonic::client::Grpc<T>,
    }

    impl PrismaTunnelClient<tonic::transport::Channel> {
        pub fn new(channel: tonic::transport::Channel) -> Self {
            let inner = tonic::client::Grpc::new(channel);
            Self { inner }
        }
    }

    impl<T> PrismaTunnelClient<T>
    where
        T: tonic::client::GrpcService<tonic::body::Body>,
        T::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
        T::ResponseBody: http_body::Body<Data = bytes::Bytes> + Send + 'static,
        <T::ResponseBody as http_body::Body>::Error:
            Into<Box<dyn std::error::Error + Send + Sync>> + Send,
    {
        pub async fn tunnel(
            &mut self,
            request: impl tonic::IntoStreamingRequest<Message = TunnelData>,
        ) -> Result<Response<Streaming<TunnelData>>, Status> {
            self.inner
                .ready()
                .await
                .map_err(|e| Status::unknown(format!("Service not ready: {}", e.into())))?;
            let codec = tonic::codec::ProstCodec::<TunnelData, TunnelData>::default();
            let path = http::uri::PathAndQuery::from_static("/tunnel.PrismaTunnel/Tunnel");
            let mut req = request.into_streaming_request();
            req.extensions_mut()
                .insert(tonic::GrpcMethod::new("tunnel.PrismaTunnel", "Tunnel"));
            self.inner.streaming(req, path, codec).await
        }
    }
}
