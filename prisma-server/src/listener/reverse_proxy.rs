use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderName, HeaderValue, Request, Response, StatusCode};
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use tracing::warn;

/// Hop-by-hop headers that must not be forwarded.
const HOP_BY_HOP: &[&str] = &[
    "connection",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "te",
    "trailers",
    "transfer-encoding",
    "upgrade",
];

#[derive(Clone)]
pub struct ProxyState {
    pub upstream: String,
}

pub async fn reverse_proxy(State(proxy): State<ProxyState>, req: Request<Body>) -> Response<Body> {
    let upstream = &proxy.upstream;

    let uri_string = format!(
        "{}{}",
        upstream.trim_end_matches('/'),
        req.uri()
            .path_and_query()
            .map(|pq| pq.as_str())
            .unwrap_or("/")
    );

    let uri = match uri_string.parse::<hyper::Uri>() {
        Ok(u) => u,
        Err(e) => {
            warn!(error = %e, "Failed to parse upstream URI");
            return Response::builder()
                .status(StatusCode::BAD_GATEWAY)
                .body(Body::from("Bad Gateway"))
                .unwrap();
        }
    };

    // Build the forwarded request
    let (mut parts, body) = req.into_parts();
    parts.uri = uri;

    // Strip hop-by-hop headers
    for hdr in HOP_BY_HOP {
        parts.headers.remove(*hdr);
    }

    // Add forwarding headers
    if let Some(host) = parts.headers.get("host").cloned() {
        parts
            .headers
            .insert(HeaderName::from_static("x-forwarded-host"), host);
    }
    parts.headers.insert(
        HeaderName::from_static("x-forwarded-proto"),
        HeaderValue::from_static("https"),
    );

    let forwarded_req = Request::from_parts(parts, body);

    let client = Client::builder(TokioExecutor::new()).build_http::<Body>();

    match client.request(forwarded_req).await {
        Ok(resp) => {
            let (parts, body) = resp.into_parts();
            Response::from_parts(parts, Body::new(body))
        }
        Err(e) => {
            warn!(error = %e, upstream = %upstream, "Reverse proxy error");
            Response::builder()
                .status(StatusCode::BAD_GATEWAY)
                .body(Body::from("Bad Gateway"))
                .unwrap()
        }
    }
}
