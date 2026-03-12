# Prisma

A next-generation encrypted proxy infrastructure suite built in Rust. Prisma implements the **PrismaVeil** wire protocol with modern cryptographic primitives, supporting both QUIC and TCP transports with local SOCKS5 and HTTP CONNECT proxy interfaces.

## Features

- **Dual transport** — QUIC (primary) with TCP fallback for UDP-blocked networks
- **Double encryption** — PrismaVeil encryption inside QUIC/TLS for defense-in-depth
- **Modern cryptography** — X25519 ECDH, BLAKE3 KDF, ChaCha20-Poly1305 / AES-256-GCM AEAD
- **HMAC-SHA256 authentication** with constant-time verification
- **Anti-replay protection** via 1024-bit sliding window
- **Random padding** on handshake messages to resist traffic fingerprinting
- **SOCKS5 proxy interface** (RFC 1928) for application compatibility
- **HTTP CONNECT proxy** for browsers and HTTP-aware clients
- **Port forwarding / reverse proxy** — expose local services through the server (frp-style)
- **DNS caching** with async resolution
- **Connection backpressure** via configurable max connection limits
- **Structured logging** (pretty or JSON) via `tracing`

## Architecture

```
prisma/
├── prisma-core/     # Shared library: crypto, protocol, config, types
├── prisma-server/   # Proxy server (TCP + QUIC inbound)
├── prisma-client/   # Proxy client (SOCKS5 + HTTP CONNECT inbound)
└── prisma-cli/      # CLI wrapper with key/cert generation
```

**Data flow — outbound proxy:**

```
Application ──SOCKS5/HTTP──▶ prisma-client ──PrismaVeil/QUIC──▶ prisma-server ──TCP──▶ Destination
```

**Data flow — port forwarding (reverse proxy):**

```
Internet ──TCP──▶ prisma-server:port ──PrismaVeil──▶ prisma-client ──TCP──▶ Local Service
```

## Quick Start

### Prerequisites

- [Rust](https://rustup.rs/) stable toolchain
- Git

### Build

```bash
git clone <repo-url> && cd prisma
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
| `port_forwards[].name` | string | — | Label for this port forward |
| `port_forwards[].local_addr` | string | — | Local service address (e.g. `127.0.0.1:3000`) |
| `port_forwards[].remote_port` | u16 | — | Port to listen on at the server |
| `logging.level` | string | `"info"` | Log level |
| `logging.format` | string | `"pretty"` | Log format |

## Port Forwarding (Reverse Proxy)

Prisma supports frp-style port forwarding, allowing you to expose local services behind NAT/firewalls through the Prisma server.

**How it works:**

1. The client establishes an encrypted PrismaVeil tunnel to the server
2. The client sends `RegisterForward` commands for each configured port
3. The server validates the port is within the allowed range and starts listening
4. When an external connection arrives at the server's forwarded port, the server sends a `ForwardConnect` message through the tunnel
5. The client opens a local TCP connection to the mapped `local_addr`
6. Data is relayed bidirectionally through the encrypted tunnel using multiplexed `stream_id`s

**Use cases:**
- Expose a local web server to the internet
- Access services behind NAT without opening firewall ports
- Secure tunneling for development and staging environments

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
│   ├── server.rs         # ServerConfig struct
│   ├── client.rs         # ClientConfig struct
│   └── validation.rs     # Config validation rules
├── crypto/
│   ├── aead.rs           # AeadCipher trait + ChaCha20/AES-256-GCM impls
│   ├── ecdh.rs           # X25519 key exchange
│   ├── kdf.rs            # BLAKE3 key derivation
│   └── padding.rs        # Random padding generation
├── error.rs              # Error types (thiserror)
├── logging.rs            # Tracing initialization
├── protocol/
│   ├── anti_replay.rs    # Sliding window replay detection
│   ├── codec.rs          # Encode/decode for all wire messages
│   ├── handshake.rs      # Client + server handshake state machines
│   └── types.rs          # Protocol message types, constants
├── types.rs              # ClientId, ProxyAddress, CipherSuite, constants
└── util.rs               # Shared helpers (hex, HMAC, framed I/O, constant-time eq)

prisma-server/src/
├── auth.rs               # AuthStore (verifies client credentials)
├── forward.rs            # Port forwarding session (multiplexed reverse proxy)
├── handler.rs            # Connection handler (handshake → proxy or forward)
├── listener/
│   ├── tcp.rs            # TCP accept loop with connection backpressure
│   └── quic.rs           # QUIC endpoint with TLS
├── outbound.rs           # TCP connect to destination
└── relay.rs              # Bidirectional encrypted relay with anti-replay

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
```

## Documentation

Full documentation is available at the [Prisma Docs site](./prisma-docs/). To build and view locally:

```bash
cd prisma-docs && npm install && npm start
```

## License

MIT
