---
sidebar_position: 6
---

# CLI Reference

The `prisma` binary provides ten subcommands for running the server and client, generating credentials, managing configs, launching the dashboard, and diagnostics.

## `prisma server`

Start the proxy server.

```bash
prisma server -c <PATH>
```

| Flag | Default | Description |
|------|---------|-------------|
| `-c, --config <PATH>` | `server.toml` | Path to the server configuration file |

If the config file is not found in the current directory, the CLI automatically searches standard locations (`/etc/prisma/`, `~/.config/prisma/`). The server starts both TCP and QUIC listeners and waits for client connections. It validates the configuration at startup and exits with an error if validation fails.

## `prisma client`

Start the proxy client.

```bash
prisma client -c <PATH>
```

| Flag | Default | Description |
|------|---------|-------------|
| `-c, --config <PATH>` | `client.toml` | Path to the client configuration file |

If the config file is not found in the current directory, the CLI automatically searches standard locations (`/etc/prisma/`, `~/.config/prisma/`). The client starts the SOCKS5 listener (and optionally the HTTP CONNECT listener), connects to the remote server, performs the PrismaVeil handshake, and begins proxying traffic.

## `prisma gen-key`

Generate a new client identity (UUID + auth secret pair).

```bash
prisma gen-key
```

No flags. Outputs a new UUID and 64-character hex secret, along with ready-to-paste TOML snippets for both server and client configs:

```
Client ID:   a1b2c3d4-e5f6-7890-abcd-ef1234567890
Auth Secret: 4f8a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a

# Add to server.toml:
[[authorized_clients]]
id = "a1b2c3d4-e5f6-7890-abcd-ef1234567890"
auth_secret = "4f8a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a"
name = "my-client"

# Add to client.toml:
[identity]
client_id = "a1b2c3d4-e5f6-7890-abcd-ef1234567890"
auth_secret = "4f8a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a"
```

## `prisma gen-cert`

Generate a self-signed TLS certificate for development use.

```bash
prisma gen-cert -o <DIR> --cn <NAME>
```

| Flag | Default | Description |
|------|---------|-------------|
| `-o, --output <DIR>` | `.` | Output directory for the certificate and key files |
| `--cn <NAME>` | `prisma-server` | Common Name for the certificate |

Generates two files in the output directory:

- `prisma-cert.pem` — self-signed X.509 certificate
- `prisma-key.pem` — private key in PEM format

Example:

```bash
prisma gen-cert -o /etc/prisma --cn my-server.example.com
```

:::warning
Self-signed certificates are for development only. For production, use a certificate from a trusted CA or Let's Encrypt. When using self-signed certificates, clients must set `skip_cert_verify = true`.
:::

## `prisma init`

Generate annotated config files with auto-generated keys.

```bash
prisma init [OPTIONS]
```

| Flag | Default | Description |
|------|---------|-------------|
| `--cdn` | — | Include CDN section pre-configured |
| `--server-only` | — | Generate only server config |
| `--client-only` | — | Generate only client config |
| `--force` | — | Overwrite existing files |

By default, generates both `server.toml` and `client.toml` with fresh UUIDs, auth secrets, and comments explaining every option. Use `--cdn` to include a fully annotated CDN transport section.

Example:

```bash
# Generate both configs with CDN section
prisma init --cdn

# Generate only the client config, overwriting if it exists
prisma init --client-only --force
```

## `prisma validate`

Validate a config file without starting the server or client.

```bash
prisma validate -c <PATH> [-t <TYPE>]
```

| Flag | Default | Description |
|------|---------|-------------|
| `-c, --config <PATH>` | — | Path to config file |
| `-t, --type <TYPE>` | `server` | Config type: `server` or `client` |

Parses the TOML file and runs all validation rules. Exits with code 0 if valid, or prints errors and exits with a non-zero code.

Example:

```bash
prisma validate -c server.toml
prisma validate -c client.toml -t client
```

## `prisma status`

Query the management API for server status.

```bash
prisma status [-u <URL>] [-t <TOKEN>]
```

| Flag | Default | Description |
|------|---------|-------------|
| `-u, --url <URL>` | `https://127.0.0.1:9090` | Management API URL |
| `-t, --token <TOKEN>` | — | Auth token for management API |

Connects to the management API and displays server health, uptime, version, and active connection count.

Example:

```bash
prisma status -u https://127.0.0.1:9090 -t your-auth-token
```

## `prisma speed-test`

Run a bandwidth measurement against the server.

```bash
prisma speed-test -s <SERVER> [OPTIONS]
```

| Flag | Default | Description |
|------|---------|-------------|
| `-s, --server <HOST:PORT>` | — | Server address |
| `-d, --duration <SECS>` | `10` | Test duration in seconds |
| `--direction <DIR>` | `both` | Direction: `download`, `upload`, or `both` |
| `-C, --config <PATH>` | `client.toml` | Client config file (for auth credentials) |

Uses the client config to authenticate and establish a tunnel, then measures throughput in the specified direction.

Example:

```bash
prisma speed-test -s my-server.example.com:8443 -d 15 --direction download
```

## `prisma dashboard`

Launch the web dashboard with auto-download and reverse proxy.

```bash
prisma dashboard [OPTIONS]
```

| Flag | Default | Description |
|------|---------|-------------|
| `--mgmt-url <URL>` | `https://127.0.0.1:9090` | Management API URL to proxy requests to |
| `--token <TOKEN>` | — | Auth token for management API |
| `--port <PORT>` | `9091` | Port to serve the dashboard on |
| `--bind <ADDR>` | `0.0.0.0` | Address to bind the dashboard server to |
| `--no-open` | — | Don't auto-open the browser |
| `--update` | — | Force re-download of dashboard assets |

On first run, downloads the latest dashboard from GitHub Releases and caches it locally (`~/.cache/prisma/dashboard/` on Linux, `~/Library/Caches/prisma/` on macOS, `%LOCALAPPDATA%\prisma\` on Windows). Starts a local server that serves the static dashboard and reverse-proxies `/api/*` requests to the management API.

On desktop systems, the browser opens automatically. On headless/VPS (SSH sessions, no `$DISPLAY`), the URL is printed instead.

Example:

```bash
# Basic usage (connects to local management API)
prisma dashboard --token your-secure-token

# Connect to remote server
prisma dashboard --mgmt-url https://my-server.com:9090 --token my-token

# Force re-download latest dashboard
prisma dashboard --update --token your-secure-token
```

## `prisma version`

Display version information, protocol version, and supported features.

```bash
prisma version
```

No flags. Outputs the Prisma version, PrismaVeil protocol version, supported ciphers, supported transports, and feature lists.
