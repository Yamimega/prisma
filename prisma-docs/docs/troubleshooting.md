---
sidebar_position: 9
---

# Troubleshooting

Common issues and their solutions.

## Authentication failed

**Symptom:** Client fails to connect with "Authentication failed" or `AcceptStatus::AuthFailed`.

**Causes:**
- `client_id` in `client.toml` does not match any entry in `server.toml`'s `authorized_clients`
- `auth_secret` does not match between client and server configs
- `auth_secret` is not valid hex (must be exactly 64 hex characters)

**Solution:**

1. Re-run `prisma gen-key` to generate a fresh key pair
2. Copy the output to both `server.toml` and `client.toml`
3. Verify the values match exactly — no extra whitespace or truncation

## TLS certificate errors

**Symptom:** Connection fails with TLS-related errors.

**Causes:**
- Certificate or key file not found at the configured path
- Certificate expired or invalid
- Client connecting with `skip_cert_verify = false` to a self-signed certificate

**Solution:**

- Verify file paths in `server.toml` are correct and files exist
- For self-signed certificates in development, set `skip_cert_verify = true` in `client.toml`
- For production, use a certificate from a trusted CA
- Regenerate certificates: `prisma gen-cert -o . --cn prisma-server`

## QUIC connection fails (UDP blocked)

**Symptom:** Client cannot connect when `transport = "quic"`, but the server is reachable via TCP.

**Causes:**
- Firewall or network blocking UDP traffic on the server port
- Some networks (corporate, hotel) block UDP entirely

**Solution:**

Switch to TCP transport in `client.toml`:

```toml
transport = "tcp"
```

The TCP transport uses TLS over TCP and provides the same encryption guarantees.

## Port forwarding denied

**Symptom:** Client logs show `ForwardReady` with `success = false`.

**Causes:**
- Port forwarding not enabled on the server (`enabled = false` or missing)
- Requested `remote_port` is outside the server's allowed range
- The port is already in use on the server

**Solution:**

1. Verify the server has port forwarding enabled:

```toml
[port_forwarding]
enabled = true
port_range_start = 10000
port_range_end = 20000
```

2. Ensure the client's `remote_port` values fall within the server's range
3. Check that the port is not already bound by another process on the server

## Connection timeout

**Symptom:** Connections are dropped after a period of inactivity.

**Cause:** The `connection_timeout_secs` setting is too low for idle connections.

**Solution:**

Increase the timeout in `server.toml`:

```toml
[performance]
connection_timeout_secs = 600  # 10 minutes
```

## XPorta session issues

**Symptom:** XPorta transport fails to connect or drops frequently.

**Causes:**
- Server `[cdn.xporta]` not enabled or paths don't match client config
- Session expired (default 300s idle timeout)
- `data_paths` / `poll_paths` overlap between client and server configs

**Solution:**

1. Verify server has XPorta enabled:

```toml
[cdn.xporta]
enabled = true
session_path = "/api/auth"
data_paths = ["/api/v1/data", "/api/v1/sync", "/api/v1/update"]
poll_paths = ["/api/v1/notifications", "/api/v1/feed", "/api/v1/events"]
```

2. Ensure `session_path`, `data_paths`, and `poll_paths` match exactly between client and server
3. Check that `encoding` is compatible (server `"json"` or `"binary"`, client can use `"auto"`)
4. For Cloudflare deployments, verify `poll_timeout_secs` is under 100 (default 55)

## Debug logging

Enable debug or trace logging to diagnose issues:

```toml
[logging]
level = "debug"   # or "trace" for maximum detail
format = "pretty"
```

Or override via environment variable without modifying the config file:

```bash
PRISMA_LOGGING_LEVEL=trace prisma server -c server.toml
```

Key things to look for in debug logs:

- Handshake step completion messages
- Connection establishment and teardown events
- Port forward registration results
- Encryption/decryption errors
