# Prisma

A next-generation encrypted proxy infrastructure suite built in Rust. Prisma implements the **PrismaVeil** wire protocol with modern cryptographic primitives, supporting both QUIC and TCP transports with local SOCKS5 and HTTP CONNECT proxy interfaces.

## Features

- **Dual transport** — QUIC (primary) with TCP fallback for UDP-blocked networks
- **Double encryption** — PrismaVeil encryption inside QUIC/TLS for defense-in-depth
- **Modern cryptography** — X25519 ECDH, BLAKE3 KDF, ChaCha20-Poly1305 / AES-256-GCM AEAD
- **HMAC-SHA256 authentication** with constant-time verification
- **Anti-replay protection** via 1024-bit sliding window
- **Random padding** on handshake messages to resist traffic fingerprinting
- **Camouflage (anti-active-detection)** — TLS-on-TCP wrapping, decoy fallback for probes, configurable ALPN
- **SOCKS5 proxy interface** (RFC 1928) for application compatibility
- **HTTP CONNECT proxy** for browsers and HTTP-aware clients
- **Port forwarding / reverse proxy** — expose local services through the server (frp-style)
- **Routing rules engine** — domain/IP/port-based allow/block rules
- **Management API** — REST + WebSocket API for live monitoring and control
- **Web dashboard** — real-time Next.js dashboard with metrics, client management, and log streaming
- **DNS caching** with async resolution
- **Connection backpressure** via configurable max connection limits
- **Structured logging** (pretty or JSON) via `tracing` with broadcast support

## Architecture

```
prisma/
├── prisma-core/       # Shared library: crypto, protocol, config, types, state
├── prisma-server/     # Proxy server (TCP + QUIC inbound)
├── prisma-client/     # Proxy client (SOCKS5 + HTTP CONNECT inbound)
├── prisma-mgmt/       # Management API (REST + WebSocket via axum)
├── prisma-cli/        # CLI wrapper with key/cert generation
├── prisma-dashboard/  # Web dashboard (Next.js + shadcn/ui)
└── prisma-docs/       # Documentation site (Docusaurus)
```

**Data flow — outbound proxy:**

```
Application ──SOCKS5/HTTP──▶ prisma-client ──PrismaVeil/QUIC──▶ prisma-server ──TCP──▶ Destination
```

**Data flow — port forwarding (reverse proxy):**

```
Internet ──TCP──▶ prisma-server:port ──PrismaVeil──▶ prisma-client ──TCP──▶ Local Service
```

**Data flow — management & dashboard:**

```
Browser ──HTTP──▶ prisma-dashboard (Next.js) ──REST/WS──▶ prisma-mgmt (axum) ──▶ ServerState
```

## Quick Start

### One-Line Install

Automatically detects your OS and architecture:

**Linux / macOS:**
```bash
curl -fsSL https://raw.githubusercontent.com/Yamimega/prisma/master/install.sh | bash
```

**Windows (PowerShell):**
```powershell
irm https://raw.githubusercontent.com/Yamimega/prisma/master/install.ps1 | iex
```

**Install + Setup** (also generates credentials, TLS certs, and example configs):

```bash
# Linux / macOS
curl -fsSL https://raw.githubusercontent.com/Yamimega/prisma/master/install.sh | bash -s -- --setup

# Windows (PowerShell)
& ([scriptblock]::Create((irm https://raw.githubusercontent.com/Yamimega/prisma/master/install.ps1))) -Setup
```

<details>
<summary>Manual platform-specific downloads</summary>

| Platform | Command |
|----------|---------|
| Linux x86_64 | `curl -fsSL https://github.com/Yamimega/prisma/releases/latest/download/prisma-linux-amd64 -o /usr/local/bin/prisma && chmod +x /usr/local/bin/prisma` |
| Linux aarch64 | `curl -fsSL https://github.com/Yamimega/prisma/releases/latest/download/prisma-linux-arm64 -o /usr/local/bin/prisma && chmod +x /usr/local/bin/prisma` |
| Linux ARMv7 | `curl -fsSL https://github.com/Yamimega/prisma/releases/latest/download/prisma-linux-armv7 -o /usr/local/bin/prisma && chmod +x /usr/local/bin/prisma` |
| macOS | `curl -fsSL https://github.com/Yamimega/prisma/releases/latest/download/prisma-darwin-$(uname -m | sed s/x86_64/amd64/) -o /usr/local/bin/prisma && chmod +x /usr/local/bin/prisma` |
| Windows x64 | `Invoke-WebRequest -Uri "https://github.com/Yamimega/prisma/releases/latest/download/prisma-windows-amd64.exe" -OutFile "$env:LOCALAPPDATA\prisma\prisma.exe"` |
| Windows ARM64 | `Invoke-WebRequest -Uri "https://github.com/Yamimega/prisma/releases/latest/download/prisma-windows-arm64.exe" -OutFile "$env:LOCALAPPDATA\prisma\prisma.exe"` |
| FreeBSD x86_64 | `fetch -o /usr/local/bin/prisma https://github.com/Yamimega/prisma/releases/latest/download/prisma-freebsd-amd64 && chmod +x /usr/local/bin/prisma` |
| Cargo (any) | `cargo install --git https://github.com/Yamimega/prisma.git prisma-cli` |
| Docker | `docker run --rm -v $(pwd):/config ghcr.io/yamimega/prisma server -c /config/server.toml` |

</details>

### Prerequisites

- [Rust](https://rustup.rs/) stable toolchain
- [Node.js](https://nodejs.org/) 18+ (for the dashboard)
- Git

### Build

```bash
git clone https://github.com/Yamimega/prisma.git && cd prisma
cargo build --release
```

Binaries are placed in `target/release/`.

Or install the CLI directly:

```bash
cargo install --path prisma-cli
```

### 1. Generate credentials

```bash
# Generate a client UUID + auth secret pair
cargo run -p prisma-cli -- gen-key
```

Output:

```
Client ID:   a1b2c3d4-e5f6-...
Auth Secret: 4f8a...  (64 hex characters)
```

### 2. Generate TLS certificate (required for QUIC)

```bash
cargo run -p prisma-cli -- gen-cert --output . --cn prisma-server
```

This creates `prisma-cert.pem` and `prisma-key.pem` in the current directory.

### 3. Configure the server

Create `server.toml`:

```toml
listen_addr = "0.0.0.0:8443"
quic_listen_addr = "0.0.0.0:8443"

[tls]
cert_path = "prisma-cert.pem"
key_path = "prisma-key.pem"

[[authorized_clients]]
id = "<client-id from gen-key>"
auth_secret = "<auth-secret from gen-key>"
name = "my-laptop"

[logging]
level = "info"       # trace | debug | info | warn | error
format = "pretty"    # pretty | json

[performance]
max_connections = 1024
connection_timeout_secs = 300

# Enable port forwarding (reverse proxy)
[port_forwarding]
enabled = true
port_range_start = 10000
port_range_end = 20000

# Enable management API (for dashboard)
[management_api]
enabled = true
listen_addr = "127.0.0.1:9090"
auth_token = "your-secure-token-here"
cors_origins = ["http://localhost:3000"]
```

### 4. Configure the client

Create `client.toml`:

```toml
socks5_listen_addr = "127.0.0.1:1080"
http_listen_addr = "127.0.0.1:8080"  # optional, remove to disable HTTP proxy
server_addr = "<server-ip>:8443"
cipher_suite = "chacha20-poly1305"   # or "aes-256-gcm"
transport = "quic"                   # or "tcp"
skip_cert_verify = false             # set true for self-signed certs in dev

[identity]
client_id = "<same client-id>"
auth_secret = "<same auth-secret>"

# Port forwarding: expose local services through the server
[[port_forwards]]
name = "my-web-app"
local_addr = "127.0.0.1:3000"
remote_port = 10080

[logging]
level = "info"
format = "pretty"
```

### 5. Run

```bash
# Terminal 1 — start server
cargo run -p prisma-cli -- server -c server.toml

# Terminal 2 — start client
cargo run -p prisma-cli -- client -c client.toml

# Terminal 3 — use it (SOCKS5)
curl --socks5 127.0.0.1:1080 https://httpbin.org/ip

# Or via HTTP proxy
curl --proxy http://127.0.0.1:8080 https://httpbin.org/ip

# Port forwarding: access local_addr:3000 from the server at port 10080
curl http://<server-ip>:10080
```

### 6. Run the dashboard (optional)

```bash
cd prisma-dashboard
npm install
# Set environment variables
export MGMT_API_URL=http://127.0.0.1:9090
export MGMT_API_TOKEN=your-secure-token-here
export ADMIN_USERNAME=admin
export ADMIN_PASSWORD=your-dashboard-password
export AUTH_SECRET=$(openssl rand -base64 32)
npm run dev
# Open http://localhost:3000
```

## Dashboard

The Prisma dashboard provides a real-time web interface for monitoring and managing the proxy server.

**Pages:**

| Page | Description |
|------|-------------|
| **Overview** | Live metrics cards, traffic chart (bytes/sec), active connections table |
| **Server** | Server health, version, config details, TLS info |
| **Clients** | Client list with enable/disable, add/remove clients at runtime |
| **Routing** | Visual routing rules editor (domain/IP/port allow/block) |
| **Logs** | Real-time log stream with level and target filtering |
| **Settings** | Edit server config, view TLS info, camouflage status |

**Tech stack:** Next.js 16, shadcn/ui, Recharts, TanStack Query, NextAuth v5

**Data sources:**
- REST API for CRUD operations (clients, routes, config)
- WebSocket for real-time push (metrics every 1s, log entries)
- Server-side API proxy to hide the management API token from the browser

## Management API

When `management_api.enabled = true` in the server config, an axum HTTP server starts on the configured address.

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/health` | Server status, uptime, version |
| `GET` | `/api/metrics` | Current metrics snapshot |
| `GET` | `/api/metrics/history` | Time-series metrics |
| `GET` | `/api/connections` | Active connections with byte counters |
| `DELETE` | `/api/connections/:id` | Force-disconnect a session |
| `GET` | `/api/clients` | Client list |
| `POST` | `/api/clients` | Generate new client credentials |
| `PUT` | `/api/clients/:id` | Update client (name, enabled) |
| `DELETE` | `/api/clients/:id` | Remove client |
| `GET` | `/api/config` | Sanitized server config |
| `PATCH` | `/api/config` | Hot-reload supported fields |
| `GET` | `/api/config/tls` | TLS certificate info |
| `GET` | `/api/forwards` | Active port forwards |
| `GET` | `/api/routes` | Routing rules |
| `POST` | `/api/routes` | Add routing rule |
| `PUT` | `/api/routes/:id` | Update routing rule |
| `DELETE` | `/api/routes/:id` | Remove routing rule |
| WS | `/api/ws/metrics` | Push metrics every 1s |
| WS | `/api/ws/logs` | Push log entries with client-side filtering |

All endpoints require `Authorization: Bearer <auth_token>`.

## CLI Reference

| Command | Flags | Description |
|---------|-------|-------------|
| `prisma server` | `-c, --config <PATH>` (default: `server.toml`) | Start the proxy server |
| `prisma client` | `-c, --config <PATH>` (default: `client.toml`) | Start the proxy client |
| `prisma gen-key` | — | Generate a new client UUID + auth secret |
| `prisma gen-cert` | `-o, --output <DIR>` (default: `.`), `--cn <NAME>` (default: `prisma-server`) | Generate self-signed TLS certificate |

## Configuration

### Config layering

Configuration is resolved in this order (later overrides earlier):

1. Compiled defaults
2. TOML config file
3. Environment variables with `PRISMA_` prefix (underscore-separated)

Example: `PRISMA_LOGGING_LEVEL=debug` overrides `logging.level`.

### Server config reference

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `listen_addr` | string | `"0.0.0.0:8443"` | TCP listen address |
| `quic_listen_addr` | string | `"0.0.0.0:8443"` | QUIC listen address |
| `tls.cert_path` | string | — | Path to TLS certificate PEM |
| `tls.key_path` | string | — | Path to TLS private key PEM |
| `authorized_clients[].id` | string | — | Client UUID |
| `authorized_clients[].auth_secret` | string | — | 64 hex char (32 byte) shared secret |
| `authorized_clients[].name` | string? | — | Optional client label |
| `logging.level` | string | `"info"` | `trace` / `debug` / `info` / `warn` / `error` |
| `logging.format` | string | `"pretty"` | `pretty` / `json` |
| `performance.max_connections` | u32 | `1024` | Max concurrent connections |
| `performance.connection_timeout_secs` | u64 | `300` | Idle connection timeout (seconds) |
| `port_forwarding.enabled` | bool | `false` | Enable port forwarding / reverse proxy |
| `port_forwarding.port_range_start` | u16 | `1024` | Minimum allowed forwarded port |
| `port_forwarding.port_range_end` | u16 | `65535` | Maximum allowed forwarded port |
| `management_api.enabled` | bool | `false` | Enable the management REST/WS API |
| `management_api.listen_addr` | string | `"127.0.0.1:9090"` | Management API bind address |
| `management_api.auth_token` | string | — | Bearer token for API authentication |
| `management_api.cors_origins` | string[] | `[]` | Allowed CORS origins |
| `camouflage.enabled` | bool | `false` | Enable camouflage (anti-active-detection) |
| `camouflage.tls_on_tcp` | bool | `false` | Wrap TCP transport in TLS |
| `camouflage.fallback_addr` | string? | — | Decoy server address for non-Prisma connections |
| `camouflage.alpn_protocols` | string[] | `["h2", "http/1.1"]` | TLS/QUIC ALPN protocols |

### Client config reference

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `socks5_listen_addr` | string | `"127.0.0.1:1080"` | Local SOCKS5 bind address |
| `http_listen_addr` | string? | — | Local HTTP CONNECT proxy bind address (optional) |
| `server_addr` | string | — | Remote Prisma server address |
| `identity.client_id` | string | — | Client UUID (must match server config) |
| `identity.auth_secret` | string | — | Shared secret (must match server config) |
| `cipher_suite` | string | `"chacha20-poly1305"` | `chacha20-poly1305` / `aes-256-gcm` |
| `transport` | string | `"quic"` | `quic` / `tcp` |
| `skip_cert_verify` | bool | `false` | Skip TLS certificate verification |
| `tls_on_tcp` | bool | `false` | Connect via TLS-wrapped TCP |
| `tls_server_name` | string? | — | TLS SNI server name (defaults to server_addr hostname) |
| `alpn_protocols` | string[] | `["h2", "http/1.1"]` | TLS/QUIC ALPN protocols |
| `port_forwards[].name` | string | — | Label for this port forward |
| `port_forwards[].local_addr` | string | — | Local service address (e.g. `127.0.0.1:3000`) |
| `port_forwards[].remote_port` | u16 | — | Port to listen on at the server |
| `logging.level` | string | `"info"` | Log level |
| `logging.format` | string | `"pretty"` | Log format |

## Camouflage (Anti-Active-Detection)

Prisma supports camouflage to resist active probing by censorship systems like the GFW. Three layers:

1. **TLS-on-TCP** — Wraps the TCP transport in TLS (reuses existing cert/key), making PrismaVeil traffic look like HTTPS
2. **Decoy fallback** — Non-Prisma connections (HTTP probes, browsers, GFW probes) are reverse-proxied to a configurable decoy website instead of being dropped
3. **ALPN customization** — QUIC/TLS ALPN protocols are configurable (default `["h2", "http/1.1"]` instead of `"prisma-v1"`)

**Server config:**

```toml
[camouflage]
enabled = true
tls_on_tcp = true
fallback_addr = "example.com:443"
alpn_protocols = ["h2", "http/1.1"]
```

**Client config:**

```toml
tls_on_tcp = true
tls_server_name = "example.com"
alpn_protocols = ["h2", "http/1.1"]
```

See [Camouflage documentation](./prisma-docs/docs/features/camouflage.md) for detailed setup instructions.

## Port Forwarding (Reverse Proxy)

Prisma supports frp-style port forwarding, allowing you to expose local services behind NAT/firewalls through the Prisma server.

**How it works:**

1. The client establishes an encrypted PrismaVeil tunnel to the server
2. The client sends `RegisterForward` commands for each configured port
3. The server validates the port is within the allowed range and starts listening
4. When an external connection arrives at the server's forwarded port, the server sends a `ForwardConnect` message through the tunnel
5. The client opens a local TCP connection to the mapped `local_addr`
6. Data is relayed bidirectionally through the encrypted tunnel using multiplexed `stream_id`s

**Server configuration** — enable forwarding and restrict the port range:

```toml
[port_forwarding]
enabled = true
port_range_start = 10000
port_range_end = 20000
```

**Client configuration** — map local services to remote ports:

```toml
[[port_forwards]]
name = "web"
local_addr = "127.0.0.1:3000"
remote_port = 10080

[[port_forwards]]
name = "api"
local_addr = "127.0.0.1:8000"
remote_port = 10081
```

Once both are running, `http://<server-ip>:10080` routes through the encrypted tunnel to `127.0.0.1:3000` on the client machine.

## Routing Rules

The routing rules engine allows you to control which destinations clients can connect to. Rules are managed at runtime via the management API or dashboard.

**Rule conditions:**
- `DomainMatch` — glob pattern (e.g. `*.google.com`)
- `DomainExact` — exact domain match
- `IpCidr` — IP range (e.g. `192.168.0.0/16`)
- `PortRange` — port range (e.g. 80–443)
- `All` — matches everything

**Rule actions:** `Allow` or `Block`

Rules are evaluated in priority order (lowest number first). The first matching rule determines the action. If no rule matches, traffic is allowed by default.

## Protocol Overview

### PrismaVeil Handshake

```
Client                                    Server
  │                                         │
  │──── ClientHello ──────────────────────▶│  (version, X25519 pubkey, timestamp, padding)
  │                                         │
  │◀──── ServerHello ─────────────────────│  (X25519 pubkey, encrypted challenge, padding)
  │                                         │
  │  Both sides: ECDH → BLAKE3 KDF → session key
  │                                         │
  │──── ClientAuth (encrypted) ───────────▶│  (client_id, HMAC-SHA256 token, cipher suite, challenge response)
  │                                         │
  │◀──── ServerAccept (encrypted) ────────│  (status, session_id)
  │                                         │
  │════ Encrypted data frames ════════════│
```

### Encrypted frame wire format

```
[nonce:12 bytes][ciphertext length:2 bytes BE][ciphertext + AEAD tag]
```

### Data frame plaintext format

```
[command:1][flags:1][stream_id:4][payload:variable]
```

Commands: `CONNECT (0x01)`, `DATA (0x02)`, `CLOSE (0x03)`, `PING (0x04)`, `PONG (0x05)`, `REGISTER_FORWARD (0x06)`, `FORWARD_READY (0x07)`, `FORWARD_CONNECT (0x08)`

### Cryptographic details

| Component | Algorithm | Purpose |
|-----------|-----------|---------|
| Key exchange | X25519 ECDH | Ephemeral shared secret per session |
| Key derivation | BLAKE3 `derive_key` | Session key from shared secret + public keys + timestamp |
| Data encryption | ChaCha20-Poly1305 or AES-256-GCM | Authenticated encryption of data frames |
| Authentication | HMAC-SHA256 | Client identity verification |
| Challenge-response | BLAKE3 hash | Proves client derived the correct session key |
| Nonce | `[direction:1][reserved:3][counter:8]` | Per-direction monotonic counter |
| Anti-replay | 1024-bit sliding bitmap | Detects replayed or out-of-order frames |

## Development

### Running tests

```bash
# All tests
cargo test --workspace

# With nextest (faster, used in CI)
cargo nextest run --workspace

# Property-based tests only
cargo test -p prisma-core --test protocol_proptest

# Snapshot tests only
cargo test -p prisma-core --test protocol_snapshots

# Integration / E2E test
cargo test -p prisma-core --test integration
```

### Test suite

| Category | Count | Description |
|----------|-------|-------------|
| Unit tests | 38 | Crypto primitives, codec round-trips, anti-replay, handshake, HTTP parsing |
| Config tests | 7 | Loading, validation, defaults, rejection of invalid configs |
| Property tests | 6 | Randomized round-trip testing via proptest |
| Snapshot tests | 6 | Wire format stability via insta |
| Integration | 1 | Full E2E: handshake + encrypted echo through tunnel |
| **Total** | **58** | |

### Dashboard development

```bash
cd prisma-dashboard
npm install
npm run dev     # Start dev server on http://localhost:3000
npm run build   # Production build
```

### Linting

```bash
cargo fmt --all -- --check
cargo clippy --workspace -- -D warnings
```

### Updating snapshots

If you change the wire format:

```bash
cargo insta test --accept
```

### Project structure

```
prisma-core/src/
├── cache.rs              # DNS cache (moka)
├── config/
│   ├── mod.rs            # Config loading (config-rs)
│   ├── server.rs         # ServerConfig + ManagementApiConfig + RoutingRule
│   ├── client.rs         # ClientConfig struct
│   └── validation.rs     # Config validation rules
├── crypto/
│   ├── aead.rs           # AeadCipher trait + ChaCha20/AES-256-GCM impls
│   ├── ecdh.rs           # X25519 key exchange
│   ├── kdf.rs            # BLAKE3 key derivation
│   └── padding.rs        # Random padding generation
├── error.rs              # Error types (thiserror)
├── logging.rs            # Tracing initialization + broadcast layer
├── protocol/
│   ├── anti_replay.rs    # Sliding window replay detection
│   ├── codec.rs          # Encode/decode for all wire messages
│   ├── handshake.rs      # Client + server handshake state machines
│   └── types.rs          # Protocol message types, constants
├── state.rs              # ServerState, ServerMetrics, ConnectionInfo, AuthStoreInner
├── types.rs              # ClientId, ProxyAddress, CipherSuite, constants
└── util.rs               # Shared helpers (hex, HMAC, framed I/O, constant-time eq)

prisma-server/src/
├── auth.rs               # AuthStore (verifies client credentials, runtime CRUD)
├── forward.rs            # Port forwarding session (multiplexed reverse proxy)
├── handler.rs            # Connection handler (handshake → routing rules → proxy or forward)
├── listener/
│   ├── tcp.rs            # TCP accept loop with connection backpressure
│   └── quic.rs           # QUIC endpoint with TLS + semaphore limit
├── outbound.rs           # TCP connect to destination
├── relay.rs              # Bidirectional encrypted relay with anti-replay + byte counting
└── state.rs              # Re-exports from prisma-core::state

prisma-mgmt/src/
├── auth.rs               # Bearer token middleware
├── handlers/
│   ├── health.rs         # GET /api/health, /api/metrics
│   ├── connections.rs    # GET /api/connections, DELETE /api/connections/:id
│   ├── clients.rs        # CRUD /api/clients
│   ├── config.rs         # GET/PATCH /api/config, GET /api/config/tls
│   ├── forwards.rs       # GET /api/forwards
│   └── routes.rs         # CRUD /api/routes
├── ws/
│   ├── metrics.rs        # WS /api/ws/metrics
│   └── logs.rs           # WS /api/ws/logs
├── router.rs             # Axum router with all routes
└── lib.rs                # pub async fn serve()

prisma-client/src/
├── connector.rs          # TCP / QUIC transport to server
├── forward.rs            # Port forwarding client (registers forwards, relays local)
├── proxy.rs              # Shared ProxyContext for all inbound protocols
├── relay.rs              # Bidirectional relay (local ↔ tunnel)
├── socks5/
│   └── server.rs         # RFC 1928 SOCKS5 implementation
├── http/
│   └── server.rs         # HTTP CONNECT proxy implementation
└── tunnel.rs             # PrismaVeil tunnel establishment

prisma-dashboard/src/
├── app/                  # Next.js App Router pages
│   ├── dashboard/        # Overview, Clients, Routing, Logs, Settings pages
│   ├── login/            # Authentication page
│   └── api/              # NextAuth + API proxy routes
├── components/           # React components (shadcn/ui + custom)
├── hooks/                # WebSocket + TanStack Query hooks
└── lib/                  # API client, types, auth config, utilities
```

## Documentation

Full documentation is available at the [Prisma Docs site](./prisma-docs/). To build and view locally:

```bash
cd prisma-docs && npm install && npm start
```

## License

GPLv3.0
