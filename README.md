# Prisma

A next-generation encrypted proxy infrastructure suite built in Rust. Prisma implements the **PrismaVeil v3** wire protocol — combining modern cryptography, multiple transport options, and advanced anti-censorship features.

## Highlights

- **PrismaVeil v3 protocol** — 1-RTT handshake, 0-RTT resumption, X25519 + BLAKE3 + ChaCha20/AES-256-GCM
- **6 transports** — QUIC, TCP, WebSocket, gRPC, XHTTP, XPorta (CDN-compatible)
- **TUN mode** — system-wide proxy via virtual network interface (Windows/Linux/macOS)
- **Anti-censorship** — Salamander UDP obfuscation, HTTP/3 masquerade, port hopping, TLS camouflage
- **Port forwarding** — frp-style reverse proxy over encrypted tunnels
- **Web dashboard** — real-time monitoring with Next.js + shadcn/ui
- **Smart DNS** — fake IP, tunnel, smart (GeoSite), and direct modes

## Quick Start

### Install

```bash
# Linux / macOS
curl -fsSL https://raw.githubusercontent.com/Yamimega/prisma/master/install.sh | bash -s -- --setup

# Windows (PowerShell)
& ([scriptblock]::Create((irm https://raw.githubusercontent.com/Yamimega/prisma/master/install.ps1))) -Setup
```

The `--setup` flag generates credentials, TLS certificates, and example config files.

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
├── prisma-core/       # Shared library: crypto, protocol, config, DNS, routing
├── prisma-server/     # Proxy server (TCP, QUIC, CDN inbound)
├── prisma-client/     # Proxy client (SOCKS5, HTTP CONNECT, TUN inbound)
├── prisma-mgmt/       # Management API (REST + WebSocket via axum)
├── prisma-cli/        # CLI with key/cert generation, init, validate
├── prisma-dashboard/  # Web dashboard (Next.js + shadcn/ui)
└── prisma-docs/       # Documentation site (Docusaurus)
```

## Documentation

Full documentation is available at **[yamimega.github.io/prisma](https://yamimega.github.io/prisma/)**, including:

- [Getting Started](https://yamimega.github.io/prisma/docs/getting-started) — first proxy session walkthrough
- [Installation](https://yamimega.github.io/prisma/docs/installation) — all platforms, Docker, Cargo
- [Server Configuration](https://yamimega.github.io/prisma/docs/configuration/server) — full config reference
- [Client Configuration](https://yamimega.github.io/prisma/docs/configuration/client) — full config reference
- [TUN Mode](https://yamimega.github.io/prisma/docs/features/tun-mode) — system-wide proxy setup
- [PrismaVeil Protocol](https://yamimega.github.io/prisma/docs/security/prismaveil-protocol) — wire protocol specification
- [XPorta Transport](https://yamimega.github.io/prisma/docs/features/xporta-transport) — CDN transport details
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
