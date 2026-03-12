---
sidebar_position: 5
---

# Dashboard

The Prisma dashboard is a real-time web interface for monitoring and managing the proxy server. It is built with Next.js 16, shadcn/ui, Recharts, and TanStack Query.

## Prerequisites

- Node.js 18+
- A running Prisma server with the [Management API](/docs/features/management-api) enabled

## Setup

```bash
cd prisma-dashboard
npm install
```

Create a `.env.local` file:

```env
# Management API connection
MGMT_API_URL=http://127.0.0.1:9090
MGMT_API_TOKEN=your-secure-token-here

# Dashboard authentication
ADMIN_USERNAME=admin
ADMIN_PASSWORD=your-dashboard-password
AUTH_SECRET=random-32-byte-secret-for-jwt
```

Generate `AUTH_SECRET`:

```bash
openssl rand -base64 32
```

## Running

```bash
# Development
npm run dev
# → http://localhost:3000

# Production build
npm run build
npm start
```

## Authentication

The dashboard uses NextAuth v5 with a credentials provider. Login credentials are configured via `ADMIN_USERNAME` and `ADMIN_PASSWORD` environment variables. Sessions use JWT tokens — no database required.

All `/dashboard/*` routes are protected by middleware. Unauthenticated users are redirected to `/login`.

## Architecture

The dashboard never exposes the management API token to the browser. All API calls go through a server-side proxy route:

```
Browser → /api/proxy/api/health → Next.js route handler → http://127.0.0.1:9090/api/health
                                   (adds Bearer token)
```

WebSocket connections for real-time metrics and logs connect directly to the management API. Configure `NEXT_PUBLIC_WS_URL` if the WebSocket endpoint differs from the proxy route.

## Pages

### Overview

The main dashboard showing:
- **Metrics cards** — active connections, total bytes up/down, uptime
- **Traffic chart** — real-time bytes/sec upload and download over time (Recharts area chart)
- **Connection table** — active connections with peer address, transport type, mode, byte counters, and a disconnect button

Data sources: WebSocket push (metrics every 1s) + REST polling (connections every 5s).

### Server

Server information:
- Health status, version, and uptime
- Server configuration details (listen addresses, max connections, timeouts)
- TLS certificate info

### Clients

Client management:
- **Client list** — shows all authorized clients with name, status (enabled/disabled), and actions
- **Add client** — generates a new UUID + auth secret pair and displays the key once
- **Edit client** — update name, toggle enabled/disabled
- **Delete client** — remove a client from the auth store

Changes take effect immediately — no server restart required.

### Routing

Visual routing rules editor:
- **Rule list** — all rules sorted by priority, showing condition, action, and enabled status
- **Rule editor** — dialog form for creating new rules with condition type, value, and action
- **Toggle/delete** — enable, disable, or remove rules inline

See [Routing Rules](/docs/features/routing-rules) for details on rule types.

### Logs

Real-time log streaming:
- **Log viewer** — scrollable, monospace log output with colored level badges
- **Filters** — filter by log level (ERROR, WARN, INFO, DEBUG, TRACE) and target string
- **Auto-scroll** — automatically follows new log entries unless the user scrolls up
- **Clear** — clear the log buffer

Data source: WebSocket push (real-time log entries).

### Settings

Server configuration editor:
- **Editable fields** — logging level, logging format, max connections, port forwarding toggle
- **Read-only fields** — listen addresses (require server restart)
- **TLS info** — certificate status and file paths
- **Camouflage** — traffic obfuscation settings (coming soon)

## Environment Variables

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `MGMT_API_URL` | No | `http://127.0.0.1:9090` | Rust management API URL |
| `MGMT_API_TOKEN` | Yes | — | Bearer token matching `management_api.auth_token` |
| `ADMIN_USERNAME` | No | `admin` | Dashboard login username |
| `ADMIN_PASSWORD` | No | `admin` | Dashboard login password |
| `AUTH_SECRET` | Yes | — | Secret for signing JWT session tokens |
| `NEXT_PUBLIC_WS_URL` | No | — | Override WebSocket URL (for custom proxy setups) |
