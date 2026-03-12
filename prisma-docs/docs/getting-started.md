---
sidebar_position: 2
---

# Getting Started

This guide walks you through building Prisma from source and running your first proxy session.

## Prerequisites

- [Rust](https://rustup.rs/) stable toolchain
- Git

## Build

```bash
git clone https://github.com/Yamimega/prisma.git && cd prisma
cargo build --release
```

Binaries are placed in `target/release/`.

## Quick Start

### 1. Generate credentials

```bash
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
level = "info"
format = "pretty"

[performance]
max_connections = 1024
connection_timeout_secs = 300
```

### 4. Configure the client

Create `client.toml`:

```toml
socks5_listen_addr = "127.0.0.1:1080"
http_listen_addr = "127.0.0.1:8080"
server_addr = "<server-ip>:8443"
cipher_suite = "chacha20-poly1305"
transport = "quic"
skip_cert_verify = true  # for self-signed certs in development

[identity]
client_id = "<same client-id>"
auth_secret = "<same auth-secret>"

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
```

### Usage examples

**SOCKS5 proxy:**

```bash
curl --socks5 127.0.0.1:1080 https://httpbin.org/ip
```

**HTTP CONNECT proxy:**

```bash
curl --proxy http://127.0.0.1:8080 https://httpbin.org/ip
```

**Browser configuration:**

Configure your browser's proxy settings to use SOCKS5 at `127.0.0.1:1080` or HTTP proxy at `127.0.0.1:8080`.
