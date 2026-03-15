---
sidebar_position: 5
---

# Dashboard

The Prisma dashboard is a real-time web interface for monitoring and managing the proxy server. It is built as a static site with Next.js 16, shadcn/ui, Recharts, and TanStack Query, and served directly by the Prisma server.

## Prerequisites

- A running Prisma server with the [Management API](/docs/features/management-api) enabled
- Dashboard static files (pre-built or built from source)

## Setup

### Using pre-built files

Download `prisma-dashboard.tar.gz` from the [latest release](https://github.com/Yamimega/prisma/releases/latest) and extract it:

```bash
mkdir -p /opt/prisma/dashboard
tar -xzf prisma-dashboard.tar.gz -C /opt/prisma/dashboard
```

### Building from source

```bash
cd prisma-dashboard
npm ci
npm run build
```

Static files are output to `prisma-dashboard/out/`.

### Server configuration

Point the server to the dashboard files in `server.toml`:

```toml
[management_api]
enabled = true
listen_addr = "0.0.0.0:9090"
auth_token = "your-secure-token-here"
dashboard_dir = "/opt/prisma/dashboard"  # or "./prisma-dashboard/out"
```

Start the server and access the dashboard at `https://your-server:9090/`.

### Using the CLI (auto-download)

The `prisma dashboard` command automatically downloads and serves the dashboard without manual setup:

```bash
prisma dashboard --mgmt-url https://127.0.0.1:9090 --token your-secure-token
```

This downloads the latest dashboard from GitHub Releases, caches it locally, and starts a local server that proxies API requests to your management API. The browser opens automatically on desktop systems.

## Authentication

The dashboard uses token-based authentication. Enter the `management_api.auth_token` from your server config on the login page. The token is stored in the browser's session storage and sent as a `Bearer` token with each API request.

All `/dashboard/*` routes are protected — unauthenticated users are redirected to `/login`.

## Architecture

The dashboard is built as a static single-page application (SPA) and served by the Prisma server's management API (axum). No separate Node.js process is needed in production.

```
Browser → prisma-server:9090 → static files (dashboard)
                              → /api/* (REST + WebSocket)
```

API calls from the dashboard go directly to the same-origin management API endpoints. WebSocket connections use a `?token=` query parameter for authentication (since the browser WebSocket API cannot send custom headers).

## Pages

### Overview

The main dashboard showing:
- **Metrics cards** — active connections, total bytes up/down, uptime
- **Traffic chart** — real-time bytes/sec with time-range selector (Live/1H/6H/24H/7D) and Mbps toggle
- **Transport pie chart** — connections grouped by transport type
- **Connection histogram** — connection duration distribution
- **Connection table** — active connections with peer address, transport type, mode, byte counters, and a disconnect button

Data sources: WebSocket push (metrics every 1s) + REST polling (connections every 5s).

### Server

Server information:
- Health status, version, and uptime
- Server configuration details (listen addresses, max connections, timeouts)
- TLS certificate info

### System

System monitoring:
- **System cards** — version, platform, PID, CPU and memory usage gauges
- **Certificate expiry** — countdown with color coding (green &gt;30d, yellow 7-30d, red &lt;7d)
- **Active listeners** — table of all listening addresses and protocols

### Clients

Client management:
- **Client list** — shows all authorized clients with name, status (enabled/disabled), clickable links to detail page
- **Client detail** — per-client bandwidth limits (editable), quota utilization bar, traffic chart, filtered connection table
- **Add client** — generates a new UUID + auth secret pair and displays the key once
- **Edit client** — update name, toggle enabled/disabled, configure bandwidth/quota limits
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
- **Filters** — filter by log level (ERROR, WARN, INFO, DEBUG, TRACE), target string, and message regex search
- **Auto-scroll** — automatically follows new log entries unless the user scrolls up
- **Clear** — clear the log buffer

Data source: WebSocket push (real-time log entries).

### Settings

Server configuration editor with tabbed sections:
- **General** — logging level, logging format, max connections, port forwarding toggle
- **Camouflage & CDN** — camouflage and CDN configuration (read-only)
- **Traffic & Performance** — traffic shaping, congestion, port hopping, DNS, anti-RTT settings
- **TLS & Security** — certificate info, transport-only cipher, protocol version, PrismaTLS status
- **Alerts** — configure alert thresholds (cert expiry, quota warning, handshake spike)

### Config Backups

Config backup and restore:
- **Backup list** — timestamped backups with name, size, and actions
- **Create backup** — create a manual snapshot of the current config
- **Restore** — restore config from a previous backup (auto-backs up current before restoring)
- **Diff viewer** — side-by-side colored diff comparing backup vs current config
- **Delete** — remove old backups

### Traffic Shaping

Traffic shaping visualization:
- **Bucket size chart** — bar chart showing padding bucket size distribution
- **Config cards** — padding mode, jitter, chaff status, coalescing window

## Additional Features

- **i18n** — full English and Simplified Chinese translations, switchable from the header
- **Theme** — dark, light, and system mode, switchable from the header
- **Global search** — Ctrl+K command palette searching pages, clients, and config keys
- **Data export** — export tables as CSV/JSON and charts as PNG
- **Alert badge** — bell icon in header showing active alerts with severity levels
- **Responsive sidebar** — collapsible sidebar (icon-only mode), mobile drawer

## Development

For local development, you can run the Next.js dev server:

```bash
cd prisma-dashboard
npm install
npm run dev
# → http://localhost:3000
```

The dev server expects the Prisma management API running on the same origin or a CORS-enabled address. Configure `cors_origins` in your server config if using a different port:

```toml
[management_api]
cors_origins = ["http://localhost:3000"]
```
