---
sidebar_position: 3
---

# Environment Variables

Prisma uses a three-layer configuration system. Values are resolved in this order, with later layers overriding earlier ones:

1. **Compiled defaults** — hardcoded in the binary
2. **TOML config file** — `server.toml` or `client.toml`
3. **Environment variables** — `PRISMA_*` prefix

## Naming convention

Environment variables use the `PRISMA_` prefix with underscores as separators for nested fields:

```
PRISMA_{SECTION}_{FIELD}
```

The mapping from TOML paths to environment variables is:

| TOML path | Environment variable |
|-----------|---------------------|
| `listen_addr` | `PRISMA_LISTEN_ADDR` |
| `quic_listen_addr` | `PRISMA_QUIC_LISTEN_ADDR` |
| `logging.level` | `PRISMA_LOGGING_LEVEL` |
| `logging.format` | `PRISMA_LOGGING_FORMAT` |
| `performance.max_connections` | `PRISMA_PERFORMANCE_MAX_CONNECTIONS` |
| `performance.connection_timeout_secs` | `PRISMA_PERFORMANCE_CONNECTION_TIMEOUT_SECS` |
| `socks5_listen_addr` | `PRISMA_SOCKS5_LISTEN_ADDR` |
| `cipher_suite` | `PRISMA_CIPHER_SUITE` |
| `transport` | `PRISMA_TRANSPORT` |
| `skip_cert_verify` | `PRISMA_SKIP_CERT_VERIFY` |

## Examples

Override the log level at runtime without modifying the config file:

```bash
PRISMA_LOGGING_LEVEL=debug prisma server -c server.toml
```

Increase the max connection limit:

```bash
PRISMA_PERFORMANCE_MAX_CONNECTIONS=2048 prisma server -c server.toml
```

Switch transport to TCP:

```bash
PRISMA_TRANSPORT=tcp prisma client -c client.toml
```

## Type parsing

Environment variable values are automatically parsed to the expected type:

- **Strings** — used as-is
- **Integers** — parsed from decimal strings (e.g. `"1024"` → `1024`)
- **Booleans** — parsed from `"true"` / `"false"`

## Limitations

Environment variables work best for scalar values (strings, numbers, booleans). Complex structures like `authorized_clients` arrays and `port_forwards` arrays are better configured via the TOML file.
