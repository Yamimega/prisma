---
sidebar_position: 1
---

# Linux systemd Deployment

This guide covers deploying Prisma as a systemd service on Linux.

## Prerequisites

- Prisma binary built or installed (see [Installation](../installation.md))
- Root access for service installation

## 1. Create a system user

```bash
sudo useradd --system --no-create-home --shell /usr/sbin/nologin prisma
```

## 2. Set up directories

```bash
sudo mkdir -p /etc/prisma
sudo chown prisma:prisma /etc/prisma
sudo chmod 750 /etc/prisma
```

## 3. Install the binary

```bash
sudo cp target/release/prisma /usr/local/bin/prisma
sudo chmod 755 /usr/local/bin/prisma
```

## 4. Add configuration files

Copy your `server.toml` and/or `client.toml` to `/etc/prisma/`:

```bash
sudo cp server.toml /etc/prisma/server.toml
sudo cp client.toml /etc/prisma/client.toml
sudo chown prisma:prisma /etc/prisma/*.toml
sudo chmod 640 /etc/prisma/*.toml
```

If using TLS certificates, copy them as well:

```bash
sudo cp prisma-cert.pem prisma-key.pem /etc/prisma/
sudo chown prisma:prisma /etc/prisma/*.pem
sudo chmod 640 /etc/prisma/*.pem
```

Update paths in `server.toml` to reference the new locations:

```toml
[tls]
cert_path = "/etc/prisma/prisma-cert.pem"
key_path = "/etc/prisma/prisma-key.pem"
```

## 5. Install systemd service files

### Server service

Copy the service file from the repository:

```bash
sudo cp deploy/systemd/prisma-server.service /etc/systemd/system/
```

Or create `/etc/systemd/system/prisma-server.service`:

```ini
[Unit]
Description=Prisma Proxy Server
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=prisma
Group=prisma
ExecStart=/usr/local/bin/prisma server -c /etc/prisma/server.toml
Restart=on-failure
RestartSec=5
LimitNOFILE=65535
ProtectSystem=strict
ProtectHome=true
PrivateTmp=true
NoNewPrivileges=true
ReadOnlyPaths=/etc/prisma
WorkingDirectory=/etc/prisma
StandardOutput=journal
StandardError=journal
SyslogIdentifier=prisma-server

[Install]
WantedBy=multi-user.target
```

### Client service

```bash
sudo cp deploy/systemd/prisma-client.service /etc/systemd/system/
```

Or create `/etc/systemd/system/prisma-client.service`:

```ini
[Unit]
Description=Prisma Proxy Client
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=prisma
Group=prisma
ExecStart=/usr/local/bin/prisma client -c /etc/prisma/client.toml
Restart=on-failure
RestartSec=5
LimitNOFILE=65535
ProtectSystem=strict
ProtectHome=true
PrivateTmp=true
NoNewPrivileges=true
ReadOnlyPaths=/etc/prisma
WorkingDirectory=/etc/prisma
StandardOutput=journal
StandardError=journal
SyslogIdentifier=prisma-client

[Install]
WantedBy=multi-user.target
```

## 6. Enable and start the service

```bash
# Reload systemd to pick up the new service files
sudo systemctl daemon-reload

# Enable the service to start on boot
sudo systemctl enable prisma-server

# Start the service
sudo systemctl start prisma-server

# Check status
sudo systemctl status prisma-server
```

For the client:

```bash
sudo systemctl daemon-reload
sudo systemctl enable prisma-client
sudo systemctl start prisma-client
sudo systemctl status prisma-client
```

## 7. View logs

```bash
# Follow server logs
sudo journalctl -u prisma-server -f

# Follow client logs
sudo journalctl -u prisma-client -f

# View recent logs
sudo journalctl -u prisma-server --since "1 hour ago"
```

## Security hardening

The provided service files include several systemd security directives:

| Directive | Effect |
|-----------|--------|
| `ProtectSystem=strict` | Mounts the entire filesystem read-only except for specific paths |
| `ProtectHome=true` | Makes `/home`, `/root`, and `/run/user` inaccessible |
| `PrivateTmp=true` | Creates a private `/tmp` mount for the service |
| `NoNewPrivileges=true` | Prevents the process from gaining new privileges |
| `ReadOnlyPaths=/etc/prisma` | Ensures config files cannot be modified by the service |
| `LimitNOFILE=65535` | Raises the file descriptor limit for high connection counts |

## Directory layout summary

```
/usr/local/bin/prisma           # Binary
/etc/prisma/server.toml         # Server configuration
/etc/prisma/client.toml         # Client configuration
/etc/prisma/prisma-cert.pem     # TLS certificate
/etc/prisma/prisma-key.pem      # TLS private key
/etc/systemd/system/prisma-server.service
/etc/systemd/system/prisma-client.service
```
