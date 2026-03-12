---
sidebar_position: 2
---

# Docker

:::info
Docker 支持正在规划中。本页概述了预期的实现方案。
:::

## 多阶段构建方案

Docker 镜像将使用多阶段构建以保持最终镜像最小化：

```dockerfile
# 阶段 1：构建
FROM rust:1.82 AS builder
WORKDIR /app
COPY . .
RUN cargo build --release -p prisma-cli

# 阶段 2：运行时
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/prisma /usr/local/bin/prisma
RUN useradd --system --no-create-home prisma
USER prisma
ENTRYPOINT ["prisma"]
```

## 预期用法

### 服务端

```bash
docker run -d \
  --name prisma-server \
  -p 8443:8443/tcp \
  -p 8443:8443/udp \
  -v /path/to/server.toml:/etc/prisma/server.toml:ro \
  -v /path/to/certs:/etc/prisma/certs:ro \
  prisma server -c /etc/prisma/server.toml
```

### 客户端

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
