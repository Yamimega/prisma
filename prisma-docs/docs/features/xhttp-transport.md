---
sidebar_position: 6
---

# XHTTP Transport

XHTTP is an HTTP-native transport mode inspired by Xray-core's XHTTP protocol. It tunnels PrismaVeil frames inside normal-looking HTTP traffic, making connections indistinguishable from regular web browsing to network observers and CDN infrastructure.

## Transport modes

### packet-up

Client sends PrismaVeil frames as individual HTTP POST requests to the upload endpoint. The server responds with data via a long-lived GET request to the download endpoint (optionally wrapped as Server-Sent Events).

```
Client ──POST /api/v1/upload──▶ Server (upload chunks)
Client ◀──GET /api/v1/pull─── Server (download stream)
```

Best for: environments with aggressive HTTP timeouts or proxies that buffer request bodies.

### stream-up

Client sends a streaming HTTP POST with a chunked request body. The server streams the response body back. Requires HTTP/1.1 chunked transfer or HTTP/2.

```
Client ──POST (streaming body)──▶ Server
Client ◀──(streaming response)─── Server
```

Best for: CDNs that support streaming request bodies (e.g., Cloudflare with gRPC or WebSocket fallback).

### stream-one

Single HTTP/2 bidirectional stream. The request body carries upload data and the response body carries download data, all on one connection.

```
Client ═══ H2 POST /api/v1/stream ═══ Server
          (bidirectional streaming)
```

Best for: environments with end-to-end HTTP/2 support. Most efficient mode.

## Server configuration

Enable the CDN listener and configure XHTTP paths:

```toml
[cdn]
enabled = true
listen_addr = "0.0.0.0:443"

# XHTTP endpoints (these are the default paths)
xhttp_upload_path = "/api/v1/upload"
xhttp_download_path = "/api/v1/pull"
xhttp_stream_path = "/api/v1/stream"

# Optional: wrap download in SSE format for better CDN compatibility
# xhttp_nosse = false

# Header obfuscation
response_server_header = "nginx"    # disguise as nginx
padding_header = true               # add X-Padding response header
# xhttp_extra_headers = [["X-Powered-By", "Express"]]

[cdn.tls]
cert_path = "origin-cert.pem"
key_path = "origin-key.pem"
```

## Client configuration

```toml
transport = "xhttp"
xhttp_mode = "stream-one"
xhttp_stream_url = "https://your-domain.com/api/v1/stream"

# For packet-up or stream-up mode:
# xhttp_mode = "packet-up"
# xhttp_upload_url = "https://your-domain.com/api/v1/upload"
# xhttp_download_url = "https://your-domain.com/api/v1/pull"

# Header obfuscation
user_agent = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36"
referer = "https://www.google.com/"
# xhttp_extra_headers = [["Accept", "text/html"]]
```

## XMUX connection pooling

XMUX randomizes connection lifecycles to prevent long-lived connection fingerprinting. Each connection in the pool gets random limits for lifetime and request count from configurable ranges.

```toml
[xmux]
max_connections_min = 1       # minimum pool size
max_connections_max = 4       # maximum pool size
max_concurrency_min = 8       # min concurrent streams per connection
max_concurrency_max = 16      # max concurrent streams per connection
max_lifetime_secs_min = 300   # min connection lifetime (5 minutes)
max_lifetime_secs_max = 600   # max connection lifetime (10 minutes)
max_requests_min = 100        # min requests before rotation
max_requests_max = 200        # max requests before rotation
```

When a connection exceeds its randomized lifetime or request count, it is gracefully closed and replaced with a new one.

## HTTP header obfuscation

Both client and server can inject headers to make traffic look like normal web browsing:

### Server-side

- **`response_server_header`**: Override the `Server` response header (e.g., `"nginx"`, `"cloudflare"`)
- **`padding_header`**: Add `X-Padding` response header with random-length value (resists response size fingerprinting)
- **`xhttp_extra_headers`**: Add arbitrary response headers (e.g., `X-Powered-By`, `X-Request-ID`)

### Client-side

- **`user_agent`**: Override `User-Agent` request header
- **`referer`**: Add `Referer` request header
- **`xhttp_extra_headers`**: Add arbitrary request headers

## Comparison with other transports

| Feature | QUIC | TCP | WebSocket | gRPC | XHTTP | XPorta |
|---------|------|-----|-----------|------|-------|--------|
| Looks like normal HTTP | No | No | Upgrade header | grpc content-type | Yes | Yes |
| CDN-compatible | No | No | Yes | Partial | Yes | Yes |
| DPI resistance | Low | Low | Medium | Medium | High | Highest |
| Active probe resistant | No | No | No | No | No | Yes |
| Per-connection overhead | Low | Low | Medium | Medium | Low (stream-one) | Low |
| Multiple upload modes | No | No | No | No | 3 modes | Upload + long-poll |
| SSE disguise | N/A | N/A | N/A | N/A | Yes | N/A |
| Header obfuscation | N/A | N/A | Basic | No | Full | Full |

:::tip
For maximum stealth against DPI, consider [XPorta transport](/docs/features/xporta-transport) — it fragments traffic into short-lived REST API-style requests that are indistinguishable from normal web application traffic.
:::
