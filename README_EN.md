# Prisma

[简体中文](./README.md) | **English**

A next-generation encrypted proxy infrastructure suite built in Rust. Prisma implements the **PrismaVeil v4** wire protocol — combining modern cryptography, multiple transport options, and advanced anti-censorship features.

## Highlights

- **PrismaVeil v4 protocol** — 1-RTT handshake, 0-RTT resumption, X25519 + BLAKE3 + ChaCha20/AES-256-GCM/Transport-Only
- **6 transports** — QUIC v2, TCP, WebSocket, gRPC, XHTTP, XPorta (CDN-compatible)
- **TUN mode** — system-wide proxy via virtual network interface (Windows/Linux/macOS)
- **GeoIP routing** — country-level smart routing via v2fly geoip.dat, on both client and server
- **PrismaTLS** — active probing resistance replacing REALITY, with browser fingerprint mimicry + dynamic mask server pool
- **Traffic shaping** — bucket padding, timing jitter, chaff injection, frame coalescing to defeat encapsulated TLS fingerprinting
- **Anti-censorship** — Salamander UDP obfuscation, HTTP/3 masquerade, port hopping, TLS camouflage, entropy camouflage
- **Port forwarding** — frp-style reverse proxy over encrypted tunnels
- **Web dashboard** — real-time monitoring with Next.js + shadcn/ui
- **Smart DNS** — fake IP, tunnel, smart (GeoSite), and direct modes

## Quick Start

### Install

```bash
# Linux / macOS
curl -fsSL https://raw.githubusercontent.com/Yamimega/prisma/master/scripts/install.sh | bash -s -- --setup

# Windows (PowerShell)
& ([scriptblock]::Create((irm https://raw.githubusercontent.com/Yamimega/prisma/master/scripts/install.ps1))) -Setup
```

The `--setup` flag generates credentials, TLS certificates, and example config files.

The installer also supports these options:

| Option | Description |
|--------|-------------|
| `--version v0.2.1` | Install a specific version |
| `--dir ~/.local/bin` | Custom install directory |
| `--config-dir DIR` | Config output directory for `--setup` |
| `--uninstall` | Remove prisma |
| `--no-verify` | Skip SHA256 checksum verification |
| `--force` | Overwrite existing installation without prompting |
| `--quiet` | Suppress informational output |

Run `install.sh --help` or `install.ps1 -Help` for the full list.

### Run

```bash
# Start server
prisma server -c server.toml

# Start client
prisma client -c client.toml

# Test
curl --socks5 127.0.0.1:1080 https://httpbin.org/ip
```

### Build from source

```bash
git clone https://github.com/Yamimega/prisma.git && cd prisma
cargo build --release
```

## Architecture

```
prisma/
├── prisma-core/       # Shared library: crypto, protocol, config, DNS, routing, GeoIP
├── prisma-server/     # Proxy server (TCP, QUIC, CDN inbound)
├── prisma-client/     # Proxy client (SOCKS5, HTTP CONNECT, TUN inbound)
├── prisma-mgmt/       # Management API (REST + WebSocket via axum)
├── prisma-cli/        # CLI with key/cert generation, init, validate
├── prisma-dashboard/  # Web dashboard (Next.js + shadcn/ui)
├── prisma-docs/       # Documentation site (Docusaurus)
└── scripts/           # Install scripts and benchmarks
```

## Documentation

Full documentation is available at **[yamimega.github.io/prisma](https://yamimega.github.io/prisma/)**, including:

- [Getting Started](https://yamimega.github.io/prisma/docs/getting-started) — first proxy session walkthrough
- [Installation](https://yamimega.github.io/prisma/docs/installation) — all platforms, Docker, Cargo
- [Server Configuration](https://yamimega.github.io/prisma/docs/configuration/server) — full config reference
- [Client Configuration](https://yamimega.github.io/prisma/docs/configuration/client) — full config reference
- [Routing Rules](https://yamimega.github.io/prisma/docs/features/routing-rules) — client/server routing + GeoIP
- [PrismaTLS](https://yamimega.github.io/prisma/docs/features/prisma-tls) — active probing resistance
- [Traffic Shaping](https://yamimega.github.io/prisma/docs/features/traffic-shaping) — anti-fingerprinting
- [TUN Mode](https://yamimega.github.io/prisma/docs/features/tun-mode) — system-wide proxy setup
- [Config Examples](https://yamimega.github.io/prisma/docs/deployment/config-examples) — 8 ready-to-use templates
- [PrismaVeil Protocol](https://yamimega.github.io/prisma/docs/security/prismaveil-protocol) — wire protocol specification
- [Dashboard](https://yamimega.github.io/prisma/docs/features/dashboard) — web UI setup
- [Management API](https://yamimega.github.io/prisma/docs/features/management-api) — REST/WebSocket API reference

## Development

```bash
# Run tests
cargo test --workspace

# Lint
cargo fmt --all -- --check
cargo clippy --workspace -- -D warnings

# Build dashboard
cd prisma-dashboard && npm ci && npm run build

# Build docs
cd prisma-docs && npm install && npm start
```

## License

GPLv3.0
