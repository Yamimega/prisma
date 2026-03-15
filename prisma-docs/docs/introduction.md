---
sidebar_position: 1
slug: /introduction
---

# Introduction

Prisma is a next-generation encrypted proxy infrastructure suite built in Rust. It implements the **PrismaVeil v4** wire protocol — combining modern cryptography, multiple transport options, and advanced anti-censorship features.

## Features

- **PrismaVeil v4 protocol** — 1-RTT handshake, 0-RTT resumption, X25519 + BLAKE3 + ChaCha20-Poly1305 / AES-256-GCM / Transport-Only cipher modes
- **6 transports** — QUIC v2, TCP, WebSocket, gRPC, XHTTP, XPorta (CDN-compatible) with auto-fallback
- **Double encryption** — PrismaVeil encryption inside QUIC/TLS for defense-in-depth
- **Modern cryptography** — X25519 ECDH, BLAKE3 KDF, ChaCha20-Poly1305 / AES-256-GCM AEAD
- **Anti-replay protection** via 1024-bit sliding window
- **TUN mode** — system-wide proxy via virtual network interface (Windows/Linux/macOS)
- **GeoIP routing** — country-level smart routing via v2fly geoip.dat, on both client and server
- **Smart DNS** — fake IP, tunnel, smart (GeoSite), and direct modes
- **Traffic shaping** — bucket padding, chaff injection, timing jitter, frame coalescing
- **PrismaTLS** — active probing resistance with padding beacon auth, mask server pool, browser fingerprint mimicry
- **Entropy camouflage** — byte distribution shaping for DPI exemption
- **Anti-censorship** — Salamander UDP obfuscation, HTTP/3 masquerade, port hopping, TLS camouflage
- **XPorta transport** — next-gen CDN transport indistinguishable from normal REST API traffic
- **SOCKS5 proxy interface** (RFC 1928) for application compatibility
- **HTTP CONNECT proxy** for browsers and HTTP-aware clients
- **Port forwarding / reverse proxy** — expose local services through the server (frp-style)
- **Routing rules engine** — domain/IP/port/GeoIP-based allow/block filtering on client and server
- **PrismaUDP** — UDP relay with FEC Reed-Solomon forward error correction
- **Congestion control** — BBR, Brutal, and Adaptive modes for QUIC
- **Management API** — REST + WebSocket API for live monitoring and control
- **Web dashboard** — real-time Next.js + shadcn/ui dashboard with metrics, client management, and log streaming
- **Per-client bandwidth and quota limits** — upload/download rate limiting with configurable quotas
- **Connection backpressure** via configurable max connection limits
- **Structured logging** (pretty or JSON) via `tracing` with broadcast support

## Architecture

Prisma is organized into six crates plus a dashboard:

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

### Data flow — outbound proxy

When used as an outbound proxy, applications connect to the local SOCKS5 or HTTP CONNECT interface. The client encrypts traffic with the PrismaVeil protocol and sends it over QUIC or TCP to the server, which forwards it to the destination.

```mermaid
graph LR
    A[Application] -->|SOCKS5 / HTTP| B[prisma-client]
    B -->|PrismaVeil / QUIC| C[prisma-server]
    C -->|TCP| D[Destination]
```

### Data flow — port forwarding (reverse proxy)

Port forwarding allows you to expose local services behind NAT/firewalls through the Prisma server. External connections arrive at the server and are relayed through the encrypted tunnel to the client's local service.

```mermaid
graph LR
    A[Internet] -->|TCP| B["prisma-server:port"]
    B -->|PrismaVeil| C[prisma-client]
    C -->|TCP| D[Local Service]
```

### Data flow — management & dashboard

The management API provides live observability and control. The dashboard communicates with the management API via a server-side proxy to keep the API token secure.

```mermaid
graph LR
    A[Browser] -->|HTTP| B["prisma-dashboard (Next.js)"]
    B -->|REST / WS| C["prisma-mgmt (axum)"]
    C --> D[ServerState]
```
