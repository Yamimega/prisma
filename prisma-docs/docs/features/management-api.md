---
sidebar_position: 4
---

# Management API

The management API provides live monitoring and control of the Prisma server via REST endpoints and WebSocket streams. It is implemented in the `prisma-mgmt` crate using [axum](https://github.com/tokio-rs/axum).

## Enabling the API

Add the `[management_api]` section to your `server.toml`:

```toml
[management_api]
enabled = true
listen_addr = "127.0.0.1:9090"
auth_token = "your-secure-token-here"
cors_origins = ["http://localhost:3000"]
```

## Authentication

All endpoints require a Bearer token in the `Authorization` header:

```bash
curl -H "Authorization: Bearer your-secure-token-here" http://127.0.0.1:9090/api/health
```

If `auth_token` is empty, authentication is disabled (development mode only).

## REST Endpoints

### Health & Metrics

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/health` | Server status, uptime, and version |
| `GET` | `/api/metrics` | Current metrics snapshot (connections, bytes, failures) |
| `GET` | `/api/metrics/history` | Time-series metrics history |

**Example:**

```bash
curl -H "Authorization: Bearer $TOKEN" http://127.0.0.1:9090/api/health
# {"status":"ok","uptime_secs":3600,"version":"0.1.0"}
```

### Connections

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/connections` | List all active connections with byte counters |
| `DELETE` | `/api/connections/:id` | Force-disconnect a session by ID |

### Clients

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/clients` | List all authorized clients |
| `POST` | `/api/clients` | Generate a new client (returns UUID + auth secret) |
| `PUT` | `/api/clients/:id` | Update client name or enabled status |
| `DELETE` | `/api/clients/:id` | Remove a client |

**Creating a client at runtime:**

```bash
curl -X POST -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"name": "new-device"}' \
  http://127.0.0.1:9090/api/clients
# {"id":"uuid","name":"new-device","auth_secret_hex":"64-char-hex"}
```

:::warning
The `auth_secret_hex` is only returned once at creation time. Store it securely.
:::

### Configuration

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/config` | Current server config (secrets redacted) |
| `PATCH` | `/api/config` | Hot-reload supported fields |
| `GET` | `/api/config/tls` | TLS certificate info |

**Hot-reloadable fields:** `logging_level`, `logging_format`, `max_connections`, `port_forwarding_enabled`

### Port Forwards

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/forwards` | List active port forward sessions |

### Routing Rules

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/routes` | List all routing rules |
| `POST` | `/api/routes` | Add a new routing rule |
| `PUT` | `/api/routes/:id` | Update an existing rule |
| `DELETE` | `/api/routes/:id` | Remove a rule |

See [Routing Rules](/docs/features/routing-rules) for details on rule conditions and actions.

## WebSocket Endpoints

### Metrics stream

```
WS /api/ws/metrics
```

Pushes a `MetricsSnapshot` JSON object every second:

```json
{
  "timestamp": "2025-01-01T00:00:00Z",
  "uptime_secs": 3600,
  "total_connections": 150,
  "active_connections": 12,
  "total_bytes_up": 1048576,
  "total_bytes_down": 5242880,
  "handshake_failures": 3
}
```

### Log stream

```
WS /api/ws/logs
```

Pushes log entries in real-time. Clients can send filter messages to reduce noise:

```json
{"level": "warn", "target": "prisma_server"}
```

Log entries:

```json
{
  "timestamp": "2025-01-01T00:00:01Z",
  "level": "INFO",
  "target": "prisma_server::handler",
  "message": "session_id=abc Handshake complete (TCP)"
}
```

Send `{"level": "", "target": ""}` to clear filters.
