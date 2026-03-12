---
sidebar_position: 2
---

# HTTP CONNECT Proxy

Prisma supports an optional HTTP CONNECT proxy interface on the client side. This allows HTTP-aware applications and browsers to tunnel HTTPS traffic through the encrypted PrismaVeil connection.

## How it works

The HTTP CONNECT method establishes a TCP tunnel through the proxy:

1. The client sends `CONNECT example.com:443 HTTP/1.1`
2. Prisma responds with `HTTP/1.1 200 Connection Established`
3. All subsequent data is relayed transparently through the encrypted tunnel

## Configuration

The HTTP CONNECT proxy is **optional**. To enable it, add `http_listen_addr` to `client.toml`:

```toml
http_listen_addr = "127.0.0.1:8080"
```

To disable the HTTP proxy, simply omit this field.

## Usage with curl

```bash
curl --proxy http://127.0.0.1:8080 https://httpbin.org/ip
```

## Usage with environment variables

Many command-line tools and applications respect the standard proxy environment variables:

```bash
export https_proxy=http://127.0.0.1:8080
export http_proxy=http://127.0.0.1:8080

# Now these tools automatically use the proxy:
curl https://example.com
wget https://example.com
```

## Usage with browsers

### Firefox

1. Open Settings > Network Settings > Manual proxy configuration
2. Set HTTP Proxy to `127.0.0.1`, Port to `8080`
3. Check **Also use this proxy for HTTPS**

### Chrome / Chromium

```bash
chromium --proxy-server="http://127.0.0.1:8080"
```

## SOCKS5 vs HTTP CONNECT

| Feature | SOCKS5 | HTTP CONNECT |
|---------|--------|-------------|
| Protocol support | Any TCP | HTTP/HTTPS |
| Application support | Broad (curl, browsers, most apps) | HTTP-aware apps |
| DNS resolution | Server-side (with `--socks5-hostname`) | Server-side |
| Configuration | Always enabled | Optional |

Both interfaces tunnel traffic through the same encrypted PrismaVeil connection. Choose based on your application's proxy support.
