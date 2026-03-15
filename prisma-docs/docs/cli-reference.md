---
sidebar_position: 6
---

# CLI Reference

The `prisma` binary provides subcommands for running the server and client, generating credentials, managing configs, launching the dashboard, and controlling a live server via the management API.

## Global flags

These flags apply to every subcommand:

| Flag | Env var | Description |
|------|---------|-------------|
| `--json` | — | Output raw JSON instead of formatted tables |
| `--mgmt-url <URL>` | `PRISMA_MGMT_URL` | Management API URL (overrides auto-detect) |
| `--mgmt-token <TOKEN>` | `PRISMA_MGMT_TOKEN` | Management API auth token (overrides auto-detect) |

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
prisma status
```

No command-specific flags. Uses the global `--mgmt-url` and `--mgmt-token` flags (or the `PRISMA_MGMT_URL` / `PRISMA_MGMT_TOKEN` environment variables).

Connects to the management API and displays server health, uptime, version, and active connection count.

Example:

```bash
prisma status --mgmt-url https://127.0.0.1:9090 --mgmt-token your-auth-token
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
| `--dir <PATH>` | — | Serve dashboard from a local directory instead of downloading |

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

## `prisma completions`

Generate shell completion scripts.

```bash
prisma completions <SHELL>
```

| Argument | Description |
|----------|-------------|
| `<SHELL>` | Shell to generate completions for: `bash`, `fish`, `zsh`, `elvish`, `powershell` |

Example:

```bash
# Bash
prisma completions bash >> ~/.bash_completion

# Zsh
prisma completions zsh > ~/.zfunc/_prisma
```

---

## Management API commands

The following commands communicate with a running server via the management API. Set `--mgmt-url` and `--mgmt-token` (or the corresponding env vars) as needed.

## `prisma clients`

Manage authorized clients.

```bash
prisma clients <SUBCOMMAND>
```

| Subcommand | Description |
|------------|-------------|
| `list` | List all authorized clients |
| `show <ID>` | Show details for a specific client |
| `create [--name NAME]` | Create a new client (auto-generates keys) |
| `delete <ID> [--yes]` | Delete a client (`--yes` skips confirmation) |
| `enable <ID>` | Enable a client |
| `disable <ID>` | Disable a client |

## `prisma connections`

Manage active connections.

```bash
prisma connections <SUBCOMMAND>
```

| Subcommand | Description |
|------------|-------------|
| `list` | List active connections |
| `disconnect <ID>` | Terminate a specific session |
| `watch [--interval N]` | Watch connections in real-time (default interval: 2s) |

## `prisma metrics`

View server metrics and system information.

```bash
prisma metrics [OPTIONS]
```

| Flag | Default | Description |
|------|---------|-------------|
| `--watch` | — | Auto-refresh metrics |
| `--history` | — | Show historical metrics |
| `--period <PERIOD>` | `1h` | History period: `1h`, `6h`, `24h`, `7d` |
| `--interval <SECS>` | `2` | Refresh interval in seconds (for `--watch`) |
| `--system` | — | Show system info instead of metrics |

## `prisma bandwidth`

Manage per-client bandwidth limits and quotas.

```bash
prisma bandwidth <SUBCOMMAND>
```

| Subcommand | Description |
|------------|-------------|
| `summary` | Show bandwidth summary for all clients |
| `get <ID>` | Show bandwidth and quota for a specific client |
| `set <ID> [--upload BPS] [--download BPS]` | Set upload/download limits in bits per second (0 = unlimited) |
| `quota <ID> [--limit BYTES]` | Get or set traffic quota in bytes |

## `prisma config`

Manage server configuration.

```bash
prisma config <SUBCOMMAND>
```

| Subcommand | Description |
|------------|-------------|
| `get` | Show current server configuration |
| `set <KEY> <VALUE>` | Update a configuration value (dotted notation, e.g., `logging.level`) |
| `tls` | Show TLS configuration |
| `backup create` | Create a new configuration backup |
| `backup list` | List all backups |
| `backup restore <NAME>` | Restore a backup |
| `backup diff <NAME>` | Show diff between a backup and current config |
| `backup delete <NAME>` | Delete a backup |

## `prisma routes`

Manage server-side routing rules.

```bash
prisma routes <SUBCOMMAND>
```

| Subcommand | Description |
|------------|-------------|
| `list` | List all routing rules |
| `create --name NAME --condition COND --action ACTION [--priority N]` | Create a routing rule |
| `update <ID> [--condition COND] [--action ACTION] [--priority N] [--name NAME]` | Update a routing rule |
| `delete <ID>` | Delete a routing rule |

Condition format: `TYPE:VALUE`, e.g. `DomainMatch:*.ads.*`, `IpCidr:10.0.0.0/8`, `PortRange:80-443`, `All`.

## `prisma logs`

Stream live server logs via WebSocket.

```bash
prisma logs [OPTIONS]
```

| Flag | Default | Description |
|------|---------|-------------|
| `--level <LEVEL>` | — | Minimum log level: `TRACE`, `DEBUG`, `INFO`, `WARN`, `ERROR` |
| `--lines <N>` | — | Maximum number of log lines to display before stopping |

## `prisma ping`

Measure handshake RTT to the server.

```bash
prisma ping [OPTIONS]
```

| Flag | Default | Description |
|------|---------|-------------|
| `-c, --config <PATH>` | `client.toml` | Client config file (for auth credentials) |
| `-s, --server <HOST:PORT>` | — | Override server address from config |
| `--count <N>` | `5` | Number of pings |
| `--interval <MS>` | `1000` | Interval between pings in milliseconds |

## `prisma test-transport`

Test all configured transports against the server and report which succeed.

```bash
prisma test-transport [OPTIONS]
```

| Flag | Default | Description |
|------|---------|-------------|
| `-c, --config <PATH>` | `client.toml` | Client config file |
