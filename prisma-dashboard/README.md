# Prisma Dashboard

Real-time web dashboard for monitoring and managing the [Prisma](https://github.com/Yamimega/prisma) proxy server.

## Prerequisites

- Node.js 18+
- A running Prisma server with the management API enabled (`management_api.enabled = true`)

## Setup

```bash
npm install
```

Create `.env.local`:

```env
MGMT_API_URL=http://127.0.0.1:9090
MGMT_API_TOKEN=your-secure-token-here
ADMIN_USERNAME=admin
ADMIN_PASSWORD=your-dashboard-password
AUTH_SECRET=$(openssl rand -base64 32)
```

## Development

```bash
npm run dev
# → http://localhost:3000
```

## Production

```bash
npm run build
npm start
```

## Pages

| Page | Description |
|------|-------------|
| **Overview** | Live metrics, traffic chart, active connections |
| **Server** | Health, config, TLS info |
| **Clients** | Add/remove/toggle clients at runtime |
| **Routing** | Visual routing rules editor |
| **Logs** | Real-time log stream with filtering |
| **Settings** | Server config editor |

## Tech Stack

- [Next.js 16](https://nextjs.org/) (App Router + Turbopack)
- [shadcn/ui](https://ui.shadcn.com/) (component library)
- [Recharts](https://recharts.org/) (traffic charts)
- [TanStack Query](https://tanstack.com/query) (data fetching)
- [NextAuth v5](https://authjs.dev/) (authentication)
