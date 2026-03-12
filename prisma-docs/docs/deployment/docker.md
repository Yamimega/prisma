---
sidebar_position: 2
---

# Docker

:::info
Docker support is planned. This page outlines the intended approach.
:::

## Multi-stage build outline

The Docker image will use a multi-stage build to keep the final image minimal:

```dockerfile
# Stage 1: Build
FROM rust:1.82 AS builder
WORKDIR /app
COPY . .
RUN cargo build --release -p prisma-cli

# Stage 2: Runtime
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/prisma /usr/local/bin/prisma
RUN useradd --system --no-create-home prisma
USER prisma
ENTRYPOINT ["prisma"]
```

## Planned usage

### Server

```bash
docker run -d \
  --name prisma-server \
  -p 8443:8443/tcp \
  -p 8443:8443/udp \
  -v /path/to/server.toml:/etc/prisma/server.toml:ro \
  -v /path/to/certs:/etc/prisma/certs:ro \
  prisma server -c /etc/prisma/server.toml
```

### Client

```bash
docker run -d \
  --name prisma-client \
  -p 1080:1080 \
  -p 8080:8080 \
  -v /path/to/client.toml:/etc/prisma/client.toml:ro \
  prisma client -c /etc/prisma/client.toml
```

## Docker Compose

```yaml
services:
  prisma-server:
    build: .
    command: server -c /etc/prisma/server.toml
    ports:
      - "8443:8443/tcp"
      - "8443:8443/udp"
    volumes:
      - ./server.toml:/etc/prisma/server.toml:ro
      - ./certs:/etc/prisma/certs:ro
    restart: unless-stopped
```
