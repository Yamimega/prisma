pub mod cdn;
pub mod grpc_tunnel;
pub mod h3_masquerade;
pub mod quic;
pub mod reverse_proxy;
pub mod tcp;
pub mod ws_tunnel;
pub mod xhttp;
pub mod xporta;

/// Extract real client IP from CDN/proxy headers.
/// Checks CF-Connecting-IP > X-Real-IP > X-Forwarded-For > socket addr.
pub fn extract_peer_ip(headers: &axum::http::HeaderMap, addr: &std::net::SocketAddr) -> String {
    if let Some(val) = headers.get("cf-connecting-ip") {
        if let Ok(ip) = val.to_str() {
            return ip.to_string();
        }
    }
    if let Some(val) = headers.get("x-real-ip") {
        if let Ok(ip) = val.to_str() {
            return ip.to_string();
        }
    }
    if let Some(val) = headers.get("x-forwarded-for") {
        if let Ok(s) = val.to_str() {
            if let Some(first) = s.split(',').next() {
                return first.trim().to_string();
            }
        }
    }
    addr.to_string()
}
