---
sidebar_position: 4
---

# 管理 API

管理 API 通过 REST 端点和 WebSocket 流提供对 Prisma 服务器的实时监控和控制。它在 `prisma-mgmt` crate 中使用 [axum](https://github.com/tokio-rs/axum) 实现。

## 启用 API

在 `server.toml` 中添加 `[management_api]` 配置段：

```toml
[management_api]
enabled = true
listen_addr = "0.0.0.0:9090"
auth_token = "your-secure-token-here"
dashboard_dir = "/opt/prisma/dashboard"  # 可选：提供构建好的控制面板
```

## 认证

所有端点都需要在 `Authorization` 头部中携带 Bearer 令牌：

```bash
curl -H "Authorization: Bearer your-secure-token-here" http://127.0.0.1:9090/api/health
```

如果 `auth_token` 为空，则禁用认证（仅限开发模式）。

## REST 端点

### 健康状态与指标

| 方法 | 路径 | 说明 |
|------|------|------|
| `GET` | `/api/health` | 服务器状态、运行时间和版本 |
| `GET` | `/api/metrics` | 当前指标快照（连接数、字节数、失败次数） |
| `GET` | `/api/metrics/history` | 时间序列指标历史 |

**示例：**

```bash
curl -H "Authorization: Bearer $TOKEN" http://127.0.0.1:9090/api/health
# {"status":"ok","uptime_secs":3600,"version":"0.1.0"}
```

### 连接

| 方法 | 路径 | 说明 |
|------|------|------|
| `GET` | `/api/connections` | 列出所有活跃连接及字节计数 |
| `DELETE` | `/api/connections/:id` | 按 ID 强制断开会话 |

### 客户端

| 方法 | 路径 | 说明 |
|------|------|------|
| `GET` | `/api/clients` | 列出所有授权客户端 |
| `POST` | `/api/clients` | 生成新客户端（返回 UUID + 认证密钥） |
| `PUT` | `/api/clients/:id` | 更新客户端名称或启用状态 |
| `DELETE` | `/api/clients/:id` | 删除客户端 |

**运行时创建客户端：**

```bash
curl -X POST -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"name": "new-device"}' \
  http://127.0.0.1:9090/api/clients
# {"id":"uuid","name":"new-device","auth_secret_hex":"64-char-hex"}
```

:::warning
`auth_secret_hex` 仅在创建时返回一次。请妥善保存。
:::

### 系统

| 方法 | 路径 | 说明 |
|------|------|------|
| `GET` | `/api/system/info` | 版本、平台、PID、CPU/内存使用率、证书到期时间、监听地址 |

### 配置

| 方法 | 路径 | 说明 |
|------|------|------|
| `GET` | `/api/config` | 当前服务器配置（所有段，敏感信息已脱敏） |
| `PATCH` | `/api/config` | 热重载支持的字段（更改前自动备份配置） |
| `GET` | `/api/config/tls` | TLS 证书信息 |

**支持热重载的字段：** `logging_level`、`logging_format`、`max_connections`、`port_forwarding_enabled`，以及所有流量整形、拥塞控制和伪装设置。

### 配置备份

| 方法 | 路径 | 说明 |
|------|------|------|
| `GET` | `/api/config/backups` | 列出带时间戳的配置备份 |
| `POST` | `/api/config/backup` | 创建手动备份 |
| `GET` | `/api/config/backups/:name` | 读取备份内容 |
| `POST` | `/api/config/backups/:name/restore` | 从备份恢复配置 |
| `DELETE` | `/api/config/backups/:name` | 删除备份 |
| `GET` | `/api/config/backups/:name/diff` | 比较备份与当前配置的差异 |

### 带宽与配额

| 方法 | 路径 | 说明 |
|------|------|------|
| `GET` | `/api/clients/:id/bandwidth` | 每客户端带宽限制 |
| `PUT` | `/api/clients/:id/bandwidth` | 更新带宽限制 |
| `GET` | `/api/clients/:id/quota` | 每客户端配额使用情况 |
| `PUT` | `/api/clients/:id/quota` | 更新配额配置 |
| `GET` | `/api/bandwidth/summary` | 所有客户端带宽/配额概览 |

### 告警

| 方法 | 路径 | 说明 |
|------|------|------|
| `GET` | `/api/alerts/config` | 告警阈值（证书到期、配额、握手失败峰值） |
| `PUT` | `/api/alerts/config` | 更新告警阈值（持久化到 `alerts.json`） |

### 端口转发

| 方法 | 路径 | 说明 |
|------|------|------|
| `GET` | `/api/forwards` | 列出活跃的端口转发会话 |

### 路由规则

| 方法 | 路径 | 说明 |
|------|------|------|
| `GET` | `/api/routes` | 列出所有路由规则 |
| `POST` | `/api/routes` | 添加新路由规则 |
| `PUT` | `/api/routes/:id` | 更新现有规则 |
| `DELETE` | `/api/routes/:id` | 删除规则 |

详见[路由规则](/docs/features/routing-rules)了解规则条件和操作。

## WebSocket 端点

### 指标流

```
WS /api/ws/metrics
```

每秒推送一个 `MetricsSnapshot` JSON 对象：

```json
{
  "timestamp": "2025-01-01T00:00:00Z",
  "uptime_secs": 3600,
  "total_connections": 150,
  "active_connections": 12,
  "total_bytes_up": 1048576,
  "total_bytes_down": 5242880,
  "handshake_failures": 3
}
```

### 日志流

```
WS /api/ws/logs
```

实时推送日志条目。客户端可以发送过滤消息以减少噪音：

```json
{"level": "warn", "target": "prisma_server"}
```

日志条目：

```json
{
  "timestamp": "2025-01-01T00:00:01Z",
  "level": "INFO",
  "target": "prisma_server::handler",
  "message": "session_id=abc Handshake complete (TCP)"
}
```

发送 `{"level": "", "target": ""}` 以清除过滤器。
