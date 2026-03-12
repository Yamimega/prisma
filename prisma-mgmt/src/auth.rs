use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::Response;

/// Bearer token to validate against, stored in request extensions by the router layer.
#[derive(Clone)]
pub struct AuthToken(pub String);

/// Bearer token authentication middleware.
/// The expected token is injected via request extensions by the outer layer.
pub async fn auth_middleware(request: Request, next: Next) -> Result<Response, StatusCode> {
    let expected = request
        .extensions()
        .get::<AuthToken>()
        .map(|t| t.0.clone())
        .unwrap_or_default();

    if expected.is_empty() {
        // No token configured — allow all (dev mode)
        return Ok(next.run(request).await);
    }

    let auth_header = request
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let provided = auth_header.strip_prefix("Bearer ").unwrap_or("");

    if provided != expected {
        return Err(StatusCode::UNAUTHORIZED);
    }

    Ok(next.run(request).await)
}
