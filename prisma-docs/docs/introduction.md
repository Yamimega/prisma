---
sidebar_position: 1
slug: /introduction
---

# Introduction

Prisma is a next-generation encrypted proxy infrastructure suite built in Rust. It implements the **PrismaVeil** wire protocol with modern cryptographic primitives, supporting both QUIC and TCP transports with local SOCKS5 and HTTP CONNECT proxy interfaces.

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

Prisma is organized into four crates:

```
prisma/
├── prisma-core/     # Shared library: crypto, protocol, config, types
├── prisma-server/   # Proxy server (TCP + QUIC inbound)
├── prisma-client/   # Proxy client (SOCKS5 + HTTP CONNECT inbound)
└── prisma-cli/      # CLI wrapper with key/cert generation
```

### Data flow — outbound proxy

When used as an outbound proxy, applications connect to the local SOCKS5 or HTTP CONNECT interface. The client encrypts traffic with the PrismaVeil protocol and sends it over QUIC or TCP to the server, which forwards it to the destination.

```
Application ──SOCKS5/HTTP──▶ prisma-client ──PrismaVeil/QUIC──▶ prisma-server ──TCP──▶ Destination
```

### Data flow — port forwarding (reverse proxy)

Port forwarding allows you to expose local services behind NAT/firewalls through the Prisma server. External connections arrive at the server and are relayed through the encrypted tunnel to the client's local service.

```
Internet ──TCP──▶ prisma-server:port ──PrismaVeil──▶ prisma-client ──TCP──▶ Local Service
```
