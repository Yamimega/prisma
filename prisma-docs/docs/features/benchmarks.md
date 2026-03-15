---
sidebar_position: 15
---

# Performance Benchmarks

Prisma Proxy includes an automated benchmark suite that compares its performance against [Xray-core](https://github.com/XTLS/Xray-core) and [sing-box](https://github.com/SagerNet/sing-box) across multiple transport configurations. Results are published weekly and available on the [interactive benchmarks page](/benchmarks).

## Methodology

All benchmarks run on **GitHub Actions** (`ubuntu-latest`) using loopback networking to eliminate network variability. Each scenario is measured over **5 runs** with the **median** taken as the result.

### Test Parameters

| Parameter       | Value                         |
|----------------|-------------------------------|
| Payload size    | 256 MB                        |
| Concurrency     | 4 parallel streams            |
| Measurement     | 5-run median                  |
| Network         | Loopback (localhost)          |
| Runner          | ubuntu-latest (GitHub Actions)|
| Schedule        | Weekly (Monday 04:00 UTC)     |

### Metrics Collected

- **Download throughput** (Mbps) — single-stream download speed via SOCKS5 proxy
- **Upload throughput** (Mbps) — single-stream upload speed
- **Latency / TTFB** (ms) — time-to-first-byte through the proxy chain
- **Handshake time** (ms) — connection establishment time
- **Concurrent throughput** (Mbps) — 4 parallel streams combined
- **CPU usage** (%) — average CPU under load
- **Memory idle/load** (KB) — resident set size at rest and under load
- **Stability** (CV%) — coefficient of variation across runs

## Test Scenarios

### Prisma Scenarios (8)

| Scenario          | Transport   | Encryption              | Notes                           |
|-------------------|-------------|-------------------------|---------------------------------|
| Prisma QUIC       | QUIC v2     | ChaCha20-Poly1305       | Default recommended config      |
| Prisma TCP+TLS    | TCP + TLS   | ChaCha20-Poly1305       | Traditional TCP with TLS 1.3    |
| Prisma (shaped)   | QUIC v2     | ChaCha20-Poly1305       | Traffic shaping enabled         |
| Prisma QUIC AES   | QUIC v2     | AES-256-GCM             | AES hardware acceleration       |
| Prisma T-Only     | TCP         | Transport-only          | Minimal encryption overhead     |
| Prisma WS+TLS     | WebSocket   | ChaCha20-Poly1305       | CDN-compatible WebSocket        |
| Prisma (bucket)   | QUIC v2     | ChaCha20-Poly1305       | Bucket padding enabled          |

### Xray Scenarios (8)

| Scenario           | Protocol       | Notes                        |
|--------------------|---------------|------------------------------|
| Xray VLESS+TLS     | VLESS          | Standard TLS configuration   |
| Xray VLESS+XTLS    | VLESS + XTLS   | Vision flow for splice       |
| Xray VMess+TLS     | VMess          | Legacy VMess protocol        |
| Xray Trojan+TLS    | Trojan         | Trojan protocol              |
| Xray SS AEAD       | Shadowsocks    | AEAD cipher suite            |
| Xray SS-2022       | Shadowsocks    | 2022 edition                 |
| Xray VLESS+WS      | VLESS + WS     | WebSocket transport          |
| Xray VLESS+gRPC    | VLESS + gRPC   | gRPC transport               |

### sing-box Scenarios (8)

| Scenario               | Protocol                      | Notes                           |
|------------------------|-------------------------------|---------------------------------|
| sing-box VLESS+TLS     | VLESS                         | Standard TLS configuration      |
| sing-box VMess+TLS     | VMess                         | VMess protocol                  |
| sing-box Trojan+TLS    | Trojan                        | Trojan protocol                 |
| sing-box SS AEAD       | Shadowsocks                   | chacha20-ietf-poly1305          |
| sing-box SS-2022       | Shadowsocks                   | 2022-blake3-aes-128-gcm        |
| sing-box VLESS+WS      | VLESS + WebSocket             | WebSocket transport             |
| sing-box Hysteria2     | Hysteria2 (QUIC)              | QUIC-based, high performance    |
| sing-box TUIC v5       | TUIC v5 (QUIC)                | QUIC-based multiplexing         |

### Baseline

A direct connection (no proxy) is measured as a reference point to quantify proxy overhead.

## Security Scoring

Each proxy scenario receives a composite security score (0-100) based on six dimensions:

| Dimension                      | Weight | Description                                      |
|-------------------------------|--------|--------------------------------------------------|
| Encryption Depth              | 25%    | Cipher strength and layering                     |
| Forward Secrecy               | 20%    | Ephemeral key exchange properties                |
| Traffic Analysis Resistance   | 20%    | Resistance to size/timing fingerprinting         |
| Protocol Detection Resistance | 15%    | Difficulty of identifying the protocol           |
| Anti-Replay                   | 10%    | Protection against replay attacks                |
| Auth Strength                 | 10%    | Authentication mechanism robustness              |

### Tier Classification

| Tier | Score Range | Label    |
|------|------------|----------|
| S    | 85-100     | Hardened |
| A    | 70-84      | Strong   |
| B    | 55-69      | Moderate |
| C    | 0-54       | Basic    |

## Viewing Results

- **[Interactive Benchmarks Page](/benchmarks)** — charts, tables, and use-case recommendations
- **[GitHub Actions](https://github.com/Yamimega/prisma/actions/workflows/benchmark.yml)** — raw CI logs and downloadable artifacts
- **Artifacts** — each CI run uploads per-scenario JSON files for programmatic analysis

## Running Locally

You can run the benchmark suite locally:

```bash
# Ensure prisma, xray, and sing-box binaries are available
export PRISMA_BIN=./prisma
export XRAY_BIN=./xray/xray
export SINGBOX_BIN=./sing-box/sing-box

# Run the benchmark
./scripts/benchmark.sh
```

Results are written to `benchmark-results/` as individual JSON files per scenario plus a `summary.md` report.
