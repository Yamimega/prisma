---
sidebar_position: 2
---

# 客户端配置

客户端通过 TOML 文件配置（默认：`client.toml`）。配置按三层解析——编译默认值、TOML 文件、环境变量。详见[环境变量](./environment-variables.md)了解覆盖机制。

## 配置参考

| 字段 | 类型 | 默认值 | 描述 |
|------|------|--------|------|
| `socks5_listen_addr` | string | `"127.0.0.1:1080"` | 本地 SOCKS5 代理绑定地址 |
| `http_listen_addr` | string? | — | 本地 HTTP CONNECT 代理绑定地址（可选） |
| `server_addr` | string | — | 远程 Prisma 服务器地址（如 `1.2.3.4:8443`） |
| `identity.client_id` | string | — | 客户端 UUID（须与服务端配置匹配） |
| `identity.auth_secret` | string | — | 64 个十六进制字符的共享密钥（须与服务端配置匹配） |
| `cipher_suite` | string | `"chacha20-poly1305"` | `chacha20-poly1305` / `aes-256-gcm` |
| `transport` | string | `"quic"` | `quic` / `tcp` |
| `skip_cert_verify` | bool | `false` | 跳过 TLS 证书验证 |
| `port_forwards[].name` | string | — | 端口转发的标签名称 |
| `port_forwards[].local_addr` | string | — | 本地服务地址（如 `127.0.0.1:3000`） |
| `port_forwards[].remote_port` | u16 | — | 在服务器端监听的端口 |
| `logging.level` | string | `"info"` | `trace` / `debug` / `info` / `warn` / `error` |
| `logging.format` | string | `"pretty"` | `pretty` / `json` |

## 完整示例

```toml
socks5_listen_addr = "127.0.0.1:1080"
http_listen_addr = "127.0.0.1:8080"  # 可选，删除此行以禁用 HTTP 代理
server_addr = "127.0.0.1:8443"
cipher_suite = "chacha20-poly1305"   # 或 "aes-256-gcm"
transport = "quic"                   # 或 "tcp"
skip_cert_verify = true              # 开发环境中使用自签名证书时设为 true

# 须与 prisma gen-key 生成的密钥匹配
[identity]
client_id = "00000000-0000-0000-0000-000000000001"
auth_secret = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"

# 端口转发（反向代理）— 通过服务器暴露本地服务
[[port_forwards]]
name = "my-web-app"
local_addr = "127.0.0.1:3000"
remote_port = 10080

[[port_forwards]]
name = "my-api"
local_addr = "127.0.0.1:8000"
remote_port = 10081

[logging]
level = "info"
format = "pretty"
```

## 验证规则

客户端配置在启动时进行验证，以下规则将被强制执行：

- `socks5_listen_addr` 不能为空
- `server_addr` 不能为空
- `identity.client_id` 不能为空
- `identity.auth_secret` 必须是有效的十六进制字符串
- `cipher_suite` 必须是以下之一：`chacha20-poly1305`、`aes-256-gcm`
- `transport` 必须是以下之一：`quic`、`tcp`
- `logging.level` 必须是以下之一：`trace`、`debug`、`info`、`warn`、`error`
- `logging.format` 必须是以下之一：`pretty`、`json`

## 传输选择

### QUIC（默认）

QUIC 基于 UDP 提供多路复用流传输，内置 TLS 1.3。这是大多数部署的推荐传输方式。

```toml
transport = "quic"
```

### TCP 备用

如果您的网络阻断了 UDP 流量，请使用 TCP 传输：

```toml
transport = "tcp"
```

TCP 传输使用 TLS over TCP，提供相同的加密保证。

## 禁用 HTTP 代理

HTTP CONNECT 代理是可选的。要禁用它，只需在配置中省略 `http_listen_addr` 字段：

```toml
socks5_listen_addr = "127.0.0.1:1080"
# http_listen_addr 未设置 — HTTP 代理已禁用
server_addr = "1.2.3.4:8443"
```

## 证书验证

在使用有效 TLS 证书的生产部署中，请保持 `skip_cert_verify` 为 `false`（默认值）。仅在开发环境中使用自签名证书时将其设为 `true`。
