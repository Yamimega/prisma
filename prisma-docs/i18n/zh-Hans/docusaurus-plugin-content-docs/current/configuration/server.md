---
sidebar_position: 1
---

# 服务端配置

服务端通过 TOML 文件配置（默认：`server.toml`）。配置按三层解析——编译默认值、TOML 文件、环境变量。详见[环境变量](./environment-variables.md)了解覆盖机制。

## 配置参考

| 字段 | 类型 | 默认值 | 描述 |
|------|------|--------|------|
| `listen_addr` | string | `"0.0.0.0:8443"` | TCP 监听地址 |
| `quic_listen_addr` | string | `"0.0.0.0:8443"` | QUIC 监听地址 |
| `tls.cert_path` | string | — | TLS 证书 PEM 文件路径 |
| `tls.key_path` | string | — | TLS 私钥 PEM 文件路径 |
| `authorized_clients[].id` | string | — | 客户端 UUID（由 `prisma gen-key` 生成） |
| `authorized_clients[].auth_secret` | string | — | 64 个十六进制字符（32 字节）共享密钥 |
| `authorized_clients[].name` | string? | — | 可选的客户端标签 |
| `logging.level` | string | `"info"` | `trace` / `debug` / `info` / `warn` / `error` |
| `logging.format` | string | `"pretty"` | `pretty` / `json` |
| `performance.max_connections` | u32 | `1024` | 最大并发连接数 |
| `performance.connection_timeout_secs` | u64 | `300` | 空闲连接超时时间（秒） |
| `port_forwarding.enabled` | bool | `false` | 启用端口转发 / 反向代理 |
| `port_forwarding.port_range_start` | u16 | `1024` | 允许转发的最小端口号 |
| `port_forwarding.port_range_end` | u16 | `65535` | 允许转发的最大端口号 |

## 完整示例

```toml
listen_addr = "0.0.0.0:8443"
quic_listen_addr = "0.0.0.0:8443"

[tls]
cert_path = "prisma-cert.pem"
key_path = "prisma-key.pem"

# 使用以下命令生成密钥：prisma gen-key
[[authorized_clients]]
id = "00000000-0000-0000-0000-000000000001"
auth_secret = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
name = "my-client"

[logging]
level = "info"       # trace | debug | info | warn | error
format = "pretty"    # pretty | json

[performance]
max_connections = 1024        # 最大并发连接数
connection_timeout_secs = 300 # 空闲超时时间（秒）

# 端口转发（反向代理）— 允许客户端暴露本地服务
[port_forwarding]
enabled = true
port_range_start = 10000
port_range_end = 20000
```

## 验证规则

服务端配置在启动时进行验证，以下规则将被强制执行：

- `listen_addr` 不能为空
- `authorized_clients` 中至少需要一个条目
- 每个 `authorized_clients[].id` 不能为空
- 每个 `authorized_clients[].auth_secret` 不能为空且必须是有效的十六进制字符串
- `logging.level` 必须是以下之一：`trace`、`debug`、`info`、`warn`、`error`
- `logging.format` 必须是以下之一：`pretty`、`json`

## TLS 配置

QUIC 传输需要 TLS。为开发环境生成自签名证书：

```bash
prisma gen-cert --output /etc/prisma --cn prisma-server
```

生产环境请使用受信任 CA 或 Let's Encrypt 颁发的证书。

## 多客户端

您可以通过添加多个 `[[authorized_clients]]` 条目来授权多个客户端：

```toml
[[authorized_clients]]
id = "client-uuid-1"
auth_secret = "hex-secret-1"
name = "laptop"

[[authorized_clients]]
id = "client-uuid-2"
auth_secret = "hex-secret-2"
name = "phone"
```
