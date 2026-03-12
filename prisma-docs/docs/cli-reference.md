---
sidebar_position: 6
---

# CLI Reference

The `prisma` binary provides four subcommands for running the server, client, and generating credentials.

## `prisma server`

Start the proxy server.

```bash
prisma server -c <PATH>
```

| Flag | Default | Description |
|------|---------|-------------|
| `-c, --config <PATH>` | `server.toml` | Path to the server configuration file |

The server starts both TCP and QUIC listeners and waits for client connections. It validates the configuration at startup and exits with an error if validation fails.

## `prisma client`

Start the proxy client.

```bash
prisma client -c <PATH>
```

| Flag | Default | Description |
|------|---------|-------------|
| `-c, --config <PATH>` | `client.toml` | Path to the client configuration file |

The client starts the SOCKS5 listener (and optionally the HTTP CONNECT listener), connects to the remote server, performs the PrismaVeil handshake, and begins proxying traffic.

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
