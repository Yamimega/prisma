---
sidebar_position: 2
---

# 快速开始

本指南将引导您从源码构建 Prisma 并运行您的第一个代理会话。

## 前置要求

- [Rust](https://rustup.rs/) 稳定版工具链
- Git

## 构建

```bash
git clone <repo-url> && cd prisma
cargo build --release
```

二进制文件将生成在 `target/release/` 目录下。

## 快速上手

### 1. 生成凭证

```bash
cargo run -p prisma-cli -- gen-key
```

输出：

```
Client ID:   a1b2c3d4-e5f6-...
Auth Secret: 4f8a...  (64 个十六进制字符)
```

### 2. 生成 TLS 证书（QUIC 必需）

```bash
cargo run -p prisma-cli -- gen-cert --output . --cn prisma-server
```

这将在当前目录创建 `prisma-cert.pem` 和 `prisma-key.pem`。

### 3. 配置服务端

创建 `server.toml`：

```toml
listen_addr = "0.0.0.0:8443"
quic_listen_addr = "0.0.0.0:8443"

[tls]
cert_path = "prisma-cert.pem"
key_path = "prisma-key.pem"

[[authorized_clients]]
id = "<gen-key 生成的 client-id>"
auth_secret = "<gen-key 生成的 auth-secret>"
name = "my-laptop"

[logging]
level = "info"
format = "pretty"

[performance]
max_connections = 1024
connection_timeout_secs = 300
```

### 4. 配置客户端

创建 `client.toml`：

```toml
socks5_listen_addr = "127.0.0.1:1080"
http_listen_addr = "127.0.0.1:8080"
server_addr = "<服务器IP>:8443"
cipher_suite = "chacha20-poly1305"
transport = "quic"
skip_cert_verify = true  # 开发环境中使用自签名证书时设为 true

[identity]
client_id = "<相同的 client-id>"
auth_secret = "<相同的 auth-secret>"

[logging]
level = "info"
format = "pretty"
```

### 5. 运行

```bash
# 终端 1 — 启动服务端
cargo run -p prisma-cli -- server -c server.toml

# 终端 2 — 启动客户端
cargo run -p prisma-cli -- client -c client.toml
```

### 使用示例

**SOCKS5 代理：**

```bash
curl --socks5 127.0.0.1:1080 https://httpbin.org/ip
```

**HTTP CONNECT 代理：**

```bash
curl --proxy http://127.0.0.1:8080 https://httpbin.org/ip
```

**浏览器配置：**

将浏览器的代理设置配置为使用 SOCKS5（`127.0.0.1:1080`）或 HTTP 代理（`127.0.0.1:8080`）。
