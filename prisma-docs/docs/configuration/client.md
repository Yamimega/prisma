---
sidebar_position: 2
---

# Client Configuration

The client is configured via a TOML file (default: `client.toml`). Configuration is resolved in three layers — compiled defaults, then TOML file, then environment variables. See [Environment Variables](./environment-variables.md) for override details.

## Configuration reference

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `socks5_listen_addr` | string | `"127.0.0.1:1080"` | Local SOCKS5 proxy bind address |
| `http_listen_addr` | string? | — | Local HTTP CONNECT proxy bind address (optional) |
| `server_addr` | string | — | Remote Prisma server address (e.g. `1.2.3.4:8443`) |
| `identity.client_id` | string | — | Client UUID (must match server config) |
| `identity.auth_secret` | string | — | 64 hex character shared secret (must match server config) |
| `cipher_suite` | string | `"chacha20-poly1305"` | `chacha20-poly1305` / `aes-256-gcm` |
| `transport` | string | `"quic"` | `quic` / `tcp` / `ws` / `grpc` / `xhttp` / `xporta` / `prisma-tls` |
| `skip_cert_verify` | bool | `false` | Skip TLS certificate verification |
| `tls_on_tcp` | bool | `false` | Connect via TLS-wrapped TCP (must match server camouflage) |
| `tls_server_name` | string? | — | TLS SNI server name override (defaults to server_addr hostname) |
| `alpn_protocols` | string[] | `["h2", "http/1.1"]` | TLS/QUIC ALPN protocols |
| `port_forwards[].name` | string | — | Human-readable label for this port forward |
| `port_forwards[].local_addr` | string | — | Local service address (e.g. `127.0.0.1:3000`) |
| `port_forwards[].remote_port` | u16 | — | Port to listen on at the server |
| `logging.level` | string | `"info"` | `trace` / `debug` / `info` / `warn` / `error` |
| `logging.format` | string | `"pretty"` | `pretty` / `json` |
| `ws_url` | string? | — | WebSocket server URL (e.g. `wss://domain.com/ws-tunnel`) |
| `ws_host` | string? | — | Override WebSocket `Host` header |
| `ws_extra_headers` | \[\[k,v\]\] | `[]` | Extra WebSocket request headers |
| `grpc_url` | string? | — | gRPC server URL |
| `xhttp_mode` | string? | — | XHTTP mode: `"packet-up"` / `"stream-up"` / `"stream-one"` |
| `xhttp_upload_url` | string? | — | XHTTP upload URL for packet-up/stream-up |
| `xhttp_download_url` | string? | — | XHTTP download URL for packet-up |
| `xhttp_stream_url` | string? | — | XHTTP stream URL for stream-one |
| `xhttp_extra_headers` | \[\[k,v\]\] | `[]` | Extra XHTTP request headers |
| `xporta.base_url` | string? | — | XPorta server base URL (e.g. `https://your-domain.com`) |
| `xporta.session_path` | string | `"/api/auth"` | XPorta session initialization endpoint |
| `xporta.data_paths` | string[] | `["/api/v1/data", ...]` | XPorta upload endpoint paths |
| `xporta.poll_paths` | string[] | `["/api/v1/notifications", ...]` | XPorta long-poll download paths |
| `xporta.encoding` | string | `"json"` | XPorta encoding: `"json"` / `"binary"` / `"auto"` |
| `xporta.poll_concurrency` | u8 | `3` | Concurrent pending poll requests (1-8) |
| `xporta.upload_concurrency` | u8 | `4` | Concurrent upload requests (1-8) |
| `xporta.max_payload_size` | u32 | `65536` | Max payload bytes per request |
| `xporta.poll_timeout_secs` | u16 | `55` | Long-poll timeout in seconds (10-90) |
| `xporta.extra_headers` | \[\[k,v\]\] | `[]` | Extra XPorta request headers |
| `xporta.cookie_name` | string | `"_sess"` | Session cookie name (must match server config) |
| `xmux.max_connections_min` | u16 | `1` | Min connections in pool |
| `xmux.max_connections_max` | u16 | `4` | Max connections in pool |
| `xmux.max_concurrency_min` | u16 | `8` | Min concurrency per connection |
| `xmux.max_concurrency_max` | u16 | `16` | Max concurrency per connection |
| `xmux.max_lifetime_secs_min` | u64 | `300` | Min connection lifetime (seconds) |
| `xmux.max_lifetime_secs_max` | u64 | `600` | Max connection lifetime (seconds) |
| `xmux.max_requests_min` | u32 | `100` | Min requests before rotation |
| `xmux.max_requests_max` | u32 | `200` | Max requests before rotation |
| `user_agent` | string? | — | Override User-Agent header |
| `referer` | string? | — | Override Referer header |
| `congestion.mode` | string | `"bbr"` | Congestion control: `"brutal"` / `"bbr"` / `"adaptive"` |
| `congestion.target_bandwidth` | string? | — | Target bandwidth for brutal/adaptive (e.g., `"100mbps"`) |
| `port_hopping.enabled` | bool | `false` | Enable QUIC port hopping |
| `port_hopping.base_port` | u16 | `10000` | Start of port range |
| `port_hopping.port_range` | u16 | `50000` | Number of ports in range |
| `port_hopping.interval_secs` | u64 | `60` | Seconds between port hops |
| `port_hopping.grace_period_secs` | u64 | `10` | Dual-port acceptance window |
| `salamander_password` | string? | — | Salamander UDP obfuscation password (QUIC only) |
| `udp_fec.enabled` | bool | `false` | Enable Forward Error Correction for UDP relay |
| `udp_fec.data_shards` | usize | `10` | Original packets per FEC group |
| `udp_fec.parity_shards` | usize | `3` | Parity packets per FEC group |
| `dns.mode` | string | `"direct"` | DNS mode: `"smart"` / `"fake"` / `"tunnel"` / `"direct"` |
| `dns.fake_ip_range` | string | `"198.18.0.0/15"` | CIDR range for fake DNS IPs |
| `dns.upstream` | string | `"8.8.8.8:53"` | Upstream DNS server |
| `dns.geosite_path` | string? | — | GeoSite database path for smart DNS mode |
| `dns.dns_listen_addr` | string | `"127.0.0.1:53"` | Local DNS server listen address |
| `routing.rules[].type` | string | — | Rule type: `domain` / `domain-suffix` / `domain-keyword` / `ip-cidr` / `geoip` / `port` / `all` |
| `routing.rules[].value` | string | — | Match value (country code for `geoip`, e.g. `"cn"`, `"private"`) |
| `routing.rules[].action` | string | `"proxy"` | Action: `"proxy"` / `"direct"` / `"block"` |
| `routing.geoip_path` | string? | — | Path to v2fly geoip.dat file for GeoIP-based routing |
| `tun.enabled` | bool | `false` | Enable TUN mode (system-wide proxy) |
| `tun.device_name` | string | `"prisma-tun0"` | TUN device name |
| `tun.mtu` | u16 | `1500` | TUN device MTU |
| `tun.include_routes` | string[] | `["0.0.0.0/0"]` | Routes to capture in TUN mode |
| `tun.exclude_routes` | string[] | `[]` | Routes to exclude (server IP auto-excluded) |
| `tun.dns` | string | `"fake"` | TUN DNS mode: `"fake"` / `"tunnel"` |
| `protocol_version` | string | `"v4"` | Protocol version (v4 only) |
| `fingerprint` | string | `"chrome"` | uTLS fingerprint: `chrome` / `firefox` / `safari` / `random` / `none` |
| `quic_version` | string | `"auto"` | QUIC version: `v2` / `v1` / `auto` |
| `transport_mode` | string | `"auto"` | Transport mode: `auto` or explicit name |
| `fallback_order` | string[] | `["quic-v2", ...]` | Transport fallback order for auto mode |
| `prisma_auth_secret` | string? | — | PrismaTLS auth secret (hex-encoded, must match server) |
| `traffic_shaping.padding_mode` | string | `"none"` | `none` / `random` / `bucket` |
| `traffic_shaping.bucket_sizes` | u16[] | `[128,256,...]` | Bucket sizes for bucket padding mode |
| `traffic_shaping.timing_jitter_ms` | u32 | `0` | Max timing jitter (ms) on handshake frames |
| `traffic_shaping.chaff_interval_ms` | u32 | `0` | Chaff injection interval (ms), 0=disabled |
| `traffic_shaping.coalesce_window_ms` | u32 | `0` | Frame coalescing window (ms) |
| `sni_slicing` | bool | `false` | SNI slicing for QUIC (fragment ClientHello across CRYPTO frames) |
| `entropy_camouflage` | bool | `false` | Entropy camouflage for Salamander/raw UDP |
| `transport_only_cipher` | bool | `false` | Use transport-only cipher (BLAKE3 MAC, no app-layer encryption). Only safe when transport provides confidentiality (TLS/QUIC). Server must also allow it. |

## Full example

```toml title="client.toml"
socks5_listen_addr = "127.0.0.1:1080"
http_listen_addr = "127.0.0.1:8080"  # optional, remove to disable HTTP proxy
server_addr = "127.0.0.1:8443"
cipher_suite = "chacha20-poly1305"   # or "aes-256-gcm"
transport = "quic"                   # or "tcp"
skip_cert_verify = true              # set true for self-signed certs in dev

# v4 features
protocol_version = "v4"
fingerprint = "chrome"        # uTLS fingerprint for ClientHello mimicry
quic_version = "auto"         # "v2", "v1", or "auto"
# prisma_auth_secret = "hex-encoded-32-bytes"   # For PrismaTLS transport

# Must match a key generated with: prisma gen-key
[identity]
client_id = "00000000-0000-0000-0000-000000000001"
auth_secret = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"

# Port forwarding (reverse proxy) — expose local services through the server
[[port_forwards]]
name = "my-web-app"
local_addr = "127.0.0.1:3000"
remote_port = 10080

[[port_forwards]]
name = "my-api"
local_addr = "127.0.0.1:8000"
remote_port = 10081

[logging]
level = "info"
format = "pretty"
```

## Validation rules

The client config is validated at startup. The following rules are enforced:

- `socks5_listen_addr` must not be empty
- `server_addr` must not be empty
- `identity.client_id` must not be empty
- `identity.auth_secret` must be valid hex
- `cipher_suite` must be one of: `chacha20-poly1305`, `aes-256-gcm`
- `transport` must be one of: `quic`, `tcp`, `ws`, `grpc`, `xhttp`, `xporta`, `prisma-tls`
- `xhttp_mode` (when transport is `xhttp`) must be one of: `packet-up`, `stream-up`, `stream-one`
- `xhttp_mode = "stream-one"` requires `xhttp_stream_url`
- `xhttp_mode = "packet-up"` or `"stream-up"` requires `xhttp_upload_url` and `xhttp_download_url`
- XMUX ranges must have min ≤ max
- `transport = "xporta"` requires `xporta.base_url` to be set
- XPorta: all paths must start with `/`
- XPorta: `data_paths` and `poll_paths` must not be empty or overlap
- XPorta: `encoding` must be one of: `json`, `binary`, `auto`
- XPorta: `poll_concurrency` must be 1-8, `upload_concurrency` must be 1-8
- XPorta: `poll_timeout_secs` must be 10-90
- `logging.level` must be one of: `trace`, `debug`, `info`, `warn`, `error`
- `logging.format` must be one of: `pretty`, `json`

## Transport selection

### QUIC (default)

QUIC provides multiplexed streams over UDP with built-in TLS 1.3. This is the recommended transport for most deployments.

```toml
transport = "quic"
```

### TCP fallback

If your network blocks UDP traffic, use the TCP transport:

```toml
transport = "tcp"
```

### PrismaTLS (active probing resistance)

PrismaTLS replaces REALITY for maximum active probing resistance on direct connections. The server is indistinguishable from a real website to active probers.

```toml
transport = "prisma-tls"
tls_server_name = "www.microsoft.com"
fingerprint = "chrome"
prisma_auth_secret = "hex-encoded-32-bytes"
```

See [PrismaTLS](/docs/features/prisma-tls) for detailed configuration.

### XPorta (maximum stealth — CDN)

Next-generation CDN transport that fragments proxy data into many short-lived REST API-style requests. Traffic is indistinguishable from a normal SPA making API calls.

```toml
transport = "xporta"

[xporta]
base_url = "https://your-domain.com"
session_path = "/api/auth"
data_paths = ["/api/v1/data", "/api/v1/sync", "/api/v1/update"]
poll_paths = ["/api/v1/notifications", "/api/v1/feed", "/api/v1/events"]
encoding = "json"
```

See [XPorta Transport](/docs/features/xporta-transport) for detailed configuration.

## Disabling HTTP proxy

The HTTP CONNECT proxy is optional. To disable it, simply omit the `http_listen_addr` field from your config:

```toml
socks5_listen_addr = "127.0.0.1:1080"
# http_listen_addr is not set — HTTP proxy disabled
server_addr = "1.2.3.4:8443"
```

## Certificate verification

For production deployments with a valid TLS certificate, keep `skip_cert_verify` set to `false` (the default). Only set it to `true` during development with self-signed certificates.
