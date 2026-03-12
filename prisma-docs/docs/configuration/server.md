---
sidebar_position: 1
---

# Server Configuration

The server is configured via a TOML file (default: `server.toml`). Configuration is resolved in three layers — compiled defaults, then TOML file, then environment variables. See [Environment Variables](./environment-variables.md) for override details.

## Configuration reference

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `listen_addr` | string | `"0.0.0.0:8443"` | TCP listen address |
| `quic_listen_addr` | string | `"0.0.0.0:8443"` | QUIC listen address |
| `tls.cert_path` | string | — | Path to TLS certificate PEM file |
| `tls.key_path` | string | — | Path to TLS private key PEM file |
| `authorized_clients[].id` | string | — | Client UUID (from `prisma gen-key`) |
| `authorized_clients[].auth_secret` | string | — | 64 hex character (32 byte) shared secret |
| `authorized_clients[].name` | string? | — | Optional human-readable client label |
| `logging.level` | string | `"info"` | `trace` / `debug` / `info` / `warn` / `error` |
| `logging.format` | string | `"pretty"` | `pretty` / `json` |
| `performance.max_connections` | u32 | `1024` | Maximum concurrent connections |
| `performance.connection_timeout_secs` | u64 | `300` | Idle connection timeout (seconds) |
| `port_forwarding.enabled` | bool | `false` | Enable port forwarding / reverse proxy |
| `port_forwarding.port_range_start` | u16 | `1024` | Minimum allowed forwarded port |
| `port_forwarding.port_range_end` | u16 | `65535` | Maximum allowed forwarded port |

## Full example

```toml
listen_addr = "0.0.0.0:8443"
quic_listen_addr = "0.0.0.0:8443"

[tls]
cert_path = "prisma-cert.pem"
key_path = "prisma-key.pem"

# Generate keys with: prisma gen-key
[[authorized_clients]]
id = "00000000-0000-0000-0000-000000000001"
auth_secret = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
name = "my-client"

[logging]
level = "info"       # trace | debug | info | warn | error
format = "pretty"    # pretty | json

[performance]
max_connections = 1024        # max concurrent connections
connection_timeout_secs = 300 # idle timeout in seconds

# Port forwarding (reverse proxy) — allow clients to expose local services
[port_forwarding]
enabled = true
port_range_start = 10000
port_range_end = 20000
```

## Validation rules

The server config is validated at startup. The following rules are enforced:

- `listen_addr` must not be empty
- At least one entry in `authorized_clients` is required
- Each `authorized_clients[].id` must not be empty
- Each `authorized_clients[].auth_secret` must not be empty and must be valid hex
- `logging.level` must be one of: `trace`, `debug`, `info`, `warn`, `error`
- `logging.format` must be one of: `pretty`, `json`

## TLS configuration

TLS is required for QUIC transport. Generate a self-signed certificate for development:

```bash
prisma gen-cert --output /etc/prisma --cn prisma-server
```

For production, use a certificate from a trusted CA or Let's Encrypt.

## Multiple clients

You can authorize multiple clients by adding additional `[[authorized_clients]]` entries:

```toml
[[authorized_clients]]
id = "client-uuid-1"
auth_secret = "hex-secret-1"
name = "laptop"

[[authorized_clients]]
id = "client-uuid-2"
auth_secret = "hex-secret-2"
name = "phone"
```
