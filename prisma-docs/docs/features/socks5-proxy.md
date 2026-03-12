---
sidebar_position: 1
---

# SOCKS5 Proxy

Prisma implements a full RFC 1928 SOCKS5 proxy interface on the client side. Applications connect to the local SOCKS5 port, and traffic is transparently tunneled through the encrypted PrismaVeil connection to the server.

## Supported features

- **IPv4** address connections
- **IPv6** address connections
- **Domain name** resolution (resolved at the server side)
- **CONNECT** command (TCP proxying)
- No authentication required on the local SOCKS5 interface (secured by local binding)

## Configuration

The SOCKS5 listener is always enabled. Configure the bind address in `client.toml`:

```toml
socks5_listen_addr = "127.0.0.1:1080"
```

By default, it binds to `127.0.0.1:1080`.

## Data flow

```
Application ──SOCKS5──▶ prisma-client ──PrismaVeil/QUIC──▶ prisma-server ──TCP──▶ Destination
```

1. The application sends a SOCKS5 CONNECT request with the destination address and port
2. The client parses the SOCKS5 request and extracts the destination (IPv4, IPv6, or domain)
3. The client sends a `CONNECT` command through the encrypted PrismaVeil tunnel
4. The server resolves the destination (if domain) and opens a TCP connection
5. Data is relayed bidirectionally through the encrypted tunnel

## Usage with curl

```bash
curl --socks5 127.0.0.1:1080 https://httpbin.org/ip
curl --socks5-hostname 127.0.0.1:1080 https://example.com
```

The `--socks5-hostname` variant sends the domain name to the server for resolution, avoiding local DNS leaks.

## Usage with browsers

### Firefox

1. Open Settings > Network Settings > Manual proxy configuration
2. Set SOCKS Host to `127.0.0.1`, Port to `1080`
3. Select **SOCKS v5**
4. Check **Proxy DNS when using SOCKS v5** to prevent DNS leaks

### Chrome / Chromium

Launch with the proxy flag:

```bash
chromium --proxy-server="socks5://127.0.0.1:1080"
```

## Usage with system-wide proxy

On Linux, many applications respect the `ALL_PROXY` environment variable:

```bash
export ALL_PROXY=socks5://127.0.0.1:1080
```
