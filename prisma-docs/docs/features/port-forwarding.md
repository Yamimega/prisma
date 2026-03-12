---
sidebar_position: 3
---

# Port Forwarding

Prisma supports frp-style port forwarding (reverse proxy), allowing you to expose local services behind NAT or firewalls through the Prisma server. All traffic flows through the encrypted PrismaVeil tunnel.

## How it works

```
Internet ──TCP──▶ prisma-server:port ──PrismaVeil──▶ prisma-client ──TCP──▶ Local Service
```

### Protocol flow

1. The client establishes an encrypted PrismaVeil tunnel to the server
2. The client sends `RegisterForward` commands for each configured port forward
3. The server validates the requested port is within the allowed range
4. The server responds with `ForwardReady` (success or failure) for each registration
5. When an external TCP connection arrives at the server's forwarded port, the server sends a `ForwardConnect` message through the tunnel
6. The client opens a local TCP connection to the mapped `local_addr` and relays data bidirectionally through the encrypted tunnel using multiplexed `stream_id`s

## Server configuration

Enable port forwarding and restrict the allowed port range:

```toml
[port_forwarding]
enabled = true
port_range_start = 10000
port_range_end = 20000
```

| Field | Default | Description |
|-------|---------|-------------|
| `enabled` | `false` | Must be `true` to allow any port forwards |
| `port_range_start` | `1024` | Minimum allowed forwarded port |
| `port_range_end` | `65535` | Maximum allowed forwarded port |

The server rejects any `RegisterForward` request for a port outside the configured range.

## Client configuration

Map local services to remote ports using `[[port_forwards]]` entries:

```toml
[[port_forwards]]
name = "web"
local_addr = "127.0.0.1:3000"
remote_port = 10080

[[port_forwards]]
name = "api"
local_addr = "127.0.0.1:8000"
remote_port = 10081
```

| Field | Description |
|-------|-------------|
| `name` | Human-readable label for this forward |
| `local_addr` | Address of the local service to expose |
| `remote_port` | Port the server will listen on for incoming connections |

## Multiple forwards

You can configure multiple port forwards in a single client config. Each maps a different local service to a different remote port:

```toml
[[port_forwards]]
name = "web-frontend"
local_addr = "127.0.0.1:3000"
remote_port = 10080

[[port_forwards]]
name = "api-server"
local_addr = "127.0.0.1:8000"
remote_port = 10081

[[port_forwards]]
name = "database-admin"
local_addr = "127.0.0.1:8888"
remote_port = 10082
```

## Use cases

- **Expose a local web server to the internet** — develop locally and share with others
- **Access services behind NAT** without opening firewall ports
- **Secure tunneling for development and staging** — all traffic encrypted end-to-end
- **Remote access to internal tools** — expose dashboards, admin panels, or APIs

## Security considerations

- Only enable port forwarding on the server if you need it (`enabled = false` by default)
- Restrict the port range to the minimum necessary (avoid using `1024-65535`)
- Each forwarded port is bound on the server's public interface — ensure firewall rules are appropriate
- The server validates that requested ports fall within the configured range before accepting
- All forwarded traffic is encrypted through the PrismaVeil tunnel
