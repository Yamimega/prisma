---
sidebar_position: 1
---

# Server Configuration

The server is configured via a TOML file (default: `server.toml`). Configuration is resolved in three layers — compiled defaults, then TOML file, then environment variables. See [Environment Variables](./environment-variables.md) for override details.

## Configuration reference

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `listen_addr` | string | `"0.0.0.0:8443"` | TCP listen address |
| `quic_listen_addr` | string | `"0.0.0.0:8443"` | QUIC listen address |
| `tls.cert_path` | string | — | Path to TLS certificate PEM file |
| `tls.key_path` | string | — | Path to TLS private key PEM file |
| `authorized_clients[].id` | string | — | Client UUID (from `prisma gen-key`) |
| `authorized_clients[].auth_secret` | string | — | 64 hex character (32 byte) shared secret |
| `authorized_clients[].name` | string? | — | Optional human-readable client label |
| `logging.level` | string | `"info"` | `trace` / `debug` / `info` / `warn` / `error` |
| `logging.format` | string | `"pretty"` | `pretty` / `json` |
| `performance.max_connections` | u32 | `1024` | Maximum concurrent connections |
| `performance.connection_timeout_secs` | u64 | `300` | Idle connection timeout (seconds) |
| `port_forwarding.enabled` | bool | `false` | Enable port forwarding / reverse proxy |
| `port_forwarding.port_range_start` | u16 | `1024` | Minimum allowed forwarded port |
| `port_forwarding.port_range_end` | u16 | `65535` | Maximum allowed forwarded port |
| `management_api.enabled` | bool | `false` | Enable the management REST/WS API |
| `management_api.listen_addr` | string | `"0.0.0.0:9090"` | Management API bind address |
| `management_api.auth_token` | string | — | Bearer token for API authentication |
| `management_api.cors_origins` | string[] | `[]` | Allowed CORS origins (for external dashboard dev) |
| `management_api.dashboard_dir` | string? | — | Path to built dashboard static files |
| `management_api.tls.cert_path` | string? | — | TLS certificate path for management API (inherits from `[tls]` if omitted) |
| `management_api.tls.key_path` | string? | — | TLS private key path for management API (inherits from `[tls]` if omitted) |
| `management_api.tls_enabled` | bool | `true` | Enable TLS on management API (auto-inherits server `[tls]` config) |
| `padding.min` | u16 | `0` | Minimum per-frame padding bytes |
| `padding.max` | u16 | `256` | Maximum per-frame padding bytes |
| `protocol_version` | string | `"v4"` | Protocol version (v4 only) |
| `prisma_tls.enabled` | bool | `false` | Enable PrismaTLS (replaces REALITY) |
| `prisma_tls.mask_servers` | array | `[]` | Mask server pool for relay |
| `prisma_tls.mask_servers[].addr` | string | — | Mask server address (e.g. `"www.microsoft.com:443"`) |
| `prisma_tls.mask_servers[].names` | string[] | `[]` | Allowed SNI names |
| `prisma_tls.auth_secret` | string | `""` | PrismaTLS auth secret (hex-encoded, 32 bytes) |
| `prisma_tls.auth_rotation_hours` | u64 | `1` | Auth key rotation interval in hours |
| `traffic_shaping.padding_mode` | string | `"none"` | `none` / `random` / `bucket` |
| `traffic_shaping.bucket_sizes` | u16[] | `[128,256,...]` | Bucket sizes for bucket padding mode |
| `traffic_shaping.timing_jitter_ms` | u32 | `0` | Max timing jitter (ms) on handshake frames |
| `traffic_shaping.chaff_interval_ms` | u32 | `0` | Chaff injection interval (ms), 0=disabled |
| `traffic_shaping.coalesce_window_ms` | u32 | `0` | Frame coalescing window (ms), 0=disabled |
| `allow_transport_only_cipher` | bool | `false` | Allow clients to use transport-only cipher (BLAKE3 MAC, no app-layer encryption). Only safe when transport provides confidentiality (TLS/QUIC). |
| `anti_rtt.enabled` | bool | `false` | Enable RTT normalization |
| `anti_rtt.normalization_ms` | u32 | `150` | Target RTT for normalization |
| `camouflage.enabled` | bool | `false` | Enable camouflage (anti-active-detection) |
| `camouflage.tls_on_tcp` | bool | `false` | Wrap TCP transport in TLS (requires `[tls]` config) |
| `camouflage.fallback_addr` | string? | — | Decoy server address for non-Prisma connections |
| `camouflage.alpn_protocols` | string[] | `["h2", "http/1.1"]` | TLS/QUIC ALPN protocols |
| `camouflage.h3_cover_site` | string? | — | Upstream URL for HTTP/3 masquerade cover site |
| `camouflage.h3_static_dir` | string? | — | Local static files directory for H3 masquerade |
| `cdn.enabled` | bool | `false` | Enable CDN transport listener (WS, gRPC, XHTTP) |
| `cdn.listen_addr` | string | `"0.0.0.0:443"` | CDN listener bind address |
| `cdn.tls.cert_path` | string? | — | CDN TLS certificate (e.g. Cloudflare Origin Certificate) |
| `cdn.tls.key_path` | string? | — | CDN TLS private key |
| `cdn.ws_tunnel_path` | string | `"/ws-tunnel"` | WebSocket tunnel endpoint path |
| `cdn.grpc_tunnel_path` | string | `"/tunnel.PrismaTunnel"` | gRPC tunnel service path |
| `cdn.cover_upstream` | string? | — | Reverse proxy upstream URL for cover traffic |
| `cdn.cover_static_dir` | string? | — | Static files directory for cover traffic |
| `cdn.trusted_proxies` | string[] | `[]` | Trusted proxy IP ranges (e.g. Cloudflare CIDRs) |
| `cdn.expose_management_api` | bool | `false` | Expose management API through CDN endpoint |
| `cdn.management_api_path` | string | `"/prisma-mgmt"` | Management API subpath on CDN |
| `cdn.xhttp_upload_path` | string | `"/api/v1/upload"` | XHTTP packet-up upload endpoint |
| `cdn.xhttp_download_path` | string | `"/api/v1/events"` | XHTTP packet-up download endpoint |
| `cdn.xhttp_stream_path` | string | `"/api/v1/stream"` | XHTTP stream-one/stream-up endpoint |
| `cdn.xhttp_mode` | string? | — | XHTTP mode: `"packet-up"` / `"stream-up"` / `"stream-one"` |
| `cdn.xhttp_nosse` | bool | `false` | Disable SSE wrapping for XHTTP download |
| `cdn.response_server_header` | string? | — | Override HTTP `Server` header |
| `cdn.padding_header` | bool | `true` | Add `X-Padding` response header |
| `cdn.enable_sse_disguise` | bool | `false` | Wrap download in SSE format |
| `cdn.xhttp_extra_headers` | \[\[k,v\]\] | `[]` | Extra response headers for disguise |
| `cdn.xporta.enabled` | bool | `false` | Enable XPorta transport |
| `cdn.xporta.session_path` | string | `"/api/auth"` | XPorta session endpoint |
| `cdn.xporta.data_paths` | string[] | `["/api/v1/data", ...]` | XPorta upload paths |
| `cdn.xporta.poll_paths` | string[] | `["/api/v1/notifications", ...]` | XPorta long-poll download paths |
| `cdn.xporta.session_timeout_secs` | u64 | `300` | Session idle timeout (seconds) |
| `cdn.xporta.max_sessions_per_client` | u16 | `8` | Max concurrent sessions per client |
| `cdn.xporta.cookie_name` | string | `"_sess"` | Session cookie name |
| `cdn.xporta.encoding` | string | `"json"` | Encoding: `"json"` / `"binary"` |
| `camouflage.salamander_password` | string? | — | Salamander UDP obfuscation password (QUIC only) |
| `dns_upstream` | string | `"8.8.8.8:53"` | Upstream DNS server for CMD_DNS_QUERY forwarding |
| `congestion.mode` | string | `"bbr"` | Congestion control: `"brutal"` / `"bbr"` / `"adaptive"` |
| `congestion.target_bandwidth` | string? | — | Target bandwidth for brutal/adaptive (e.g., `"100mbps"`) |
| `port_hopping.enabled` | bool | `false` | Enable QUIC port hopping |
| `port_hopping.base_port` | u16 | `10000` | Start of port range |
| `port_hopping.port_range` | u16 | `50000` | Number of ports in range |
| `port_hopping.interval_secs` | u64 | `60` | Seconds between port hops |
| `port_hopping.grace_period_secs` | u64 | `10` | Seconds to accept on old port after hop |
| `authorized_clients[].bandwidth_up` | string? | — | Per-client upload rate limit (e.g., `"100mbps"`) |
| `authorized_clients[].bandwidth_down` | string? | — | Per-client download rate limit |
| `authorized_clients[].quota` | string? | — | Per-client transfer quota (e.g., `"100GB"`) |
| `authorized_clients[].quota_period` | string? | — | Quota period: `"daily"` / `"weekly"` / `"monthly"` |
| `routing.rules[].type` | string | — | Rule type: `domain` / `domain-suffix` / `domain-keyword` / `ip-cidr` / `geoip` / `port` / `all` |
| `routing.rules[].value` | string | — | Match value |
| `routing.rules[].action` | string | — | Action: `"allow"` / `"block"` (or `"proxy"` / `"direct"` mapped to allow) |
| `routing.geoip_path` | string? | — | Path to v2fly geoip.dat file for GeoIP-based routing |

## Full example

```toml title="server.toml"
listen_addr = "0.0.0.0:8443"
quic_listen_addr = "0.0.0.0:8443"

[tls]
cert_path = "prisma-cert.pem"
key_path = "prisma-key.pem"

# Generate keys with: prisma gen-key
[[authorized_clients]]
id = "00000000-0000-0000-0000-000000000001"
auth_secret = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
name = "my-client"

[logging]
level = "info"       # trace | debug | info | warn | error
format = "pretty"    # pretty | json

[performance]
max_connections = 1024        # max concurrent connections
connection_timeout_secs = 300 # idle timeout in seconds

# Port forwarding (reverse proxy) — allow clients to expose local services
[port_forwarding]
enabled = true
port_range_start = 10000
port_range_end = 20000

# Management API + dashboard
[management_api]
enabled = true
listen_addr = "127.0.0.1:9090"
auth_token = "your-secure-token-here"
dashboard_dir = "/opt/prisma/dashboard"  # Path to built dashboard static files

# Per-frame padding
[padding]
min = 0
max = 256

# Camouflage (anti-active-detection)
[camouflage]
enabled = true
tls_on_tcp = true
fallback_addr = "example.com:443"
alpn_protocols = ["h2", "http/1.1"]
# salamander_password = "shared-obfuscation-key"  # Salamander UDP obfuscation (QUIC)
# h3_cover_site = "https://example.com"           # HTTP/3 masquerade cover site
# h3_static_dir = "/var/www/html"                 # OR serve local static files for H3

# PrismaTLS (replaces REALITY for active probing resistance)
# [prisma_tls]
# enabled = true
# auth_secret = "hex-encoded-32-bytes"
# auth_rotation_hours = 1
# [[prisma_tls.mask_servers]]
# addr = "www.microsoft.com:443"
# names = ["www.microsoft.com"]
# [[prisma_tls.mask_servers]]
# addr = "www.apple.com:443"
# names = ["www.apple.com"]

# Traffic shaping (anti-fingerprinting)
# [traffic_shaping]
# padding_mode = "bucket"
# bucket_sizes = [128, 256, 512, 1024, 2048, 4096, 8192, 16384]
# timing_jitter_ms = 30
# chaff_interval_ms = 500
# coalesce_window_ms = 5

# CDN transport (WebSocket + gRPC + XHTTP through Cloudflare)
# [cdn]
# enabled = true
# listen_addr = "0.0.0.0:443"
# ws_tunnel_path = "/ws-tunnel"
# grpc_tunnel_path = "/tunnel.PrismaTunnel"
# cover_upstream = "http://127.0.0.1:3000"        # Reverse proxy to real website
# trusted_proxies = ["173.245.48.0/20"]            # Cloudflare IP ranges
# [cdn.tls]
# cert_path = "origin-cert.pem"
# key_path = "origin-key.pem"

# XPorta transport (next-gen REST API simulation)
# [cdn.xporta]
# enabled = true
# session_path = "/api/auth"
# data_paths = ["/api/v1/data", "/api/v1/sync", "/api/v1/update"]
# poll_paths = ["/api/v1/notifications", "/api/v1/feed", "/api/v1/events"]
# session_timeout_secs = 300
# cookie_name = "_sess"
# encoding = "json"

# Static routing rules (persist across restarts)
# [routing]
# geoip_path = "/etc/prisma/geoip.dat"
# [[routing.rules]]
# type = "ip-cidr"
# value = "10.0.0.0/8"
# action = "block"
# [[routing.rules]]
# type = "domain-keyword"
# value = "torrent"
# action = "block"
# [[routing.rules]]
# type = "all"
# action = "allow"
```

## Validation rules

The server config is validated at startup. The following rules are enforced:

- `listen_addr` must not be empty
- At least one entry in `authorized_clients` is required
- Each `authorized_clients[].id` must not be empty
- Each `authorized_clients[].auth_secret` must not be empty and must be valid hex
- `logging.level` must be one of: `trace`, `debug`, `info`, `warn`, `error`
- `logging.format` must be one of: `pretty`, `json`
- `camouflage.tls_on_tcp = true` requires `tls.cert_path` and `tls.key_path` to be set

## TLS configuration

TLS is required for QUIC transport. Generate a self-signed certificate for development:

```bash
prisma gen-cert --output /etc/prisma --cn prisma-server
```

For production, use a certificate from a trusted CA or Let's Encrypt.

## Multiple clients

You can authorize multiple clients by adding additional `[[authorized_clients]]` entries:

```toml
[[authorized_clients]]
id = "client-uuid-1"
auth_secret = "hex-secret-1"
name = "laptop"

[[authorized_clients]]
id = "client-uuid-2"
auth_secret = "hex-secret-2"
name = "phone"
```

Clients can also be managed at runtime via the [Management API](/docs/features/management-api) or the [Dashboard](/docs/features/dashboard) without restarting the server.

## Management API configuration

The management API is disabled by default. When enabled, it starts an HTTP server (axum) that serves both REST endpoints and WebSocket connections.

:::warning
The `auth_token` protects all management API endpoints. Use a strong, random token in production.
:::

**Bind address**: By default the API listens on `127.0.0.1:9090` (localhost only). To expose it to the network, change `listen_addr` — but ensure you have proper network-level access controls in place.

**Dashboard**: Set `dashboard_dir` to the path containing the built dashboard static files. The server will serve the dashboard at the management API address. Download pre-built files from the [latest release](https://github.com/Yamimega/prisma/releases/latest) or build from source with `cd prisma-dashboard && npm ci && npm run build`.

**CORS origins**: Only needed when running the dashboard dev server on a different origin (e.g. `http://localhost:3000`). Not needed in production when the dashboard is served by the server itself.
