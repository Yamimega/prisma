---
sidebar_position: 3
---

# 端口转发

Prisma 支持 frp 风格的端口转发（反向代理），允许您通过 Prisma 服务器暴露 NAT 或防火墙后面的本地服务。所有流量都通过加密的 PrismaVeil 隧道传输。

## 工作原理

```
互联网 ──TCP──▶ prisma-server:port ──PrismaVeil──▶ prisma-client ──TCP──▶ 本地服务
```

### 协议流程

1. 客户端与服务端建立加密的 PrismaVeil 隧道
2. 客户端为每个配置的端口转发发送 `RegisterForward` 命令
3. 服务端验证请求的端口是否在允许范围内
4. 服务端对每个注册返回 `ForwardReady`（成功或失败）响应
5. 当外部 TCP 连接到达服务端的转发端口时，服务端通过隧道发送 `ForwardConnect` 消息
6. 客户端打开到映射 `local_addr` 的本地 TCP 连接，并通过加密隧道使用多路复用的 `stream_id` 双向中继数据

## 服务端配置

启用端口转发并限制允许的端口范围：

```toml
[port_forwarding]
enabled = true
port_range_start = 10000
port_range_end = 20000
```

| 字段 | 默认值 | 描述 |
|------|--------|------|
| `enabled` | `false` | 必须为 `true` 才能允许端口转发 |
| `port_range_start` | `1024` | 允许转发的最小端口号 |
| `port_range_end` | `65535` | 允许转发的最大端口号 |

服务端会拒绝任何请求超出配置范围端口的 `RegisterForward` 请求。

## 客户端配置

使用 `[[port_forwards]]` 条目将本地服务映射到远程端口：

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

| 字段 | 描述 |
|------|------|
| `name` | 此转发的标签名称 |
| `local_addr` | 要暴露的本地服务地址 |
| `remote_port` | 服务器端监听的端口 |

## 多端口转发

您可以在单个客户端配置中配置多个端口转发。每个转发将不同的本地服务映射到不同的远程端口：

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

## 使用场景

- **将本地 Web 服务器暴露到互联网** — 本地开发并与他人共享
- **访问 NAT 后面的服务** — 无需开放防火墙端口
- **开发和预发布环境的安全隧道** — 所有流量端到端加密
- **远程访问内部工具** — 暴露仪表盘、管理面板或 API

## 安全注意事项

- 仅在需要时在服务端启用端口转发（默认 `enabled = false`）
- 将端口范围限制在必要的最小范围（避免使用 `1024-65535`）
- 每个转发端口在服务器的公共接口上绑定——请确保防火墙规则适当
- 服务端在接受之前验证请求的端口是否在配置范围内
- 所有转发流量都通过 PrismaVeil 隧道加密传输
