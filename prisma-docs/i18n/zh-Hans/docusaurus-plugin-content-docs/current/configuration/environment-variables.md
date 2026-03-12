---
sidebar_position: 3
---

# 环境变量

Prisma 使用三层配置系统。值按以下顺序解析，后面的层覆盖前面的：

1. **编译默认值** — 硬编码在二进制文件中
2. **TOML 配置文件** — `server.toml` 或 `client.toml`
3. **环境变量** — `PRISMA_*` 前缀

## 命名规则

环境变量使用 `PRISMA_` 前缀，嵌套字段用下划线分隔：

```
PRISMA_{SECTION}_{FIELD}
```

TOML 路径到环境变量的映射：

| TOML 路径 | 环境变量 |
|-----------|----------|
| `listen_addr` | `PRISMA_LISTEN_ADDR` |
| `quic_listen_addr` | `PRISMA_QUIC_LISTEN_ADDR` |
| `logging.level` | `PRISMA_LOGGING_LEVEL` |
| `logging.format` | `PRISMA_LOGGING_FORMAT` |
| `performance.max_connections` | `PRISMA_PERFORMANCE_MAX_CONNECTIONS` |
| `performance.connection_timeout_secs` | `PRISMA_PERFORMANCE_CONNECTION_TIMEOUT_SECS` |
| `socks5_listen_addr` | `PRISMA_SOCKS5_LISTEN_ADDR` |
| `cipher_suite` | `PRISMA_CIPHER_SUITE` |
| `transport` | `PRISMA_TRANSPORT` |
| `skip_cert_verify` | `PRISMA_SKIP_CERT_VERIFY` |

## 示例

在不修改配置文件的情况下覆盖日志级别：

```bash
PRISMA_LOGGING_LEVEL=debug prisma server -c server.toml
```

增加最大连接数限制：

```bash
PRISMA_PERFORMANCE_MAX_CONNECTIONS=2048 prisma server -c server.toml
```

切换传输方式为 TCP：

```bash
PRISMA_TRANSPORT=tcp prisma client -c client.toml
```

## 类型解析

环境变量的值会自动解析为预期的类型：

- **字符串** — 直接使用
- **整数** — 从十进制字符串解析（如 `"1024"` → `1024`）
- **布尔值** — 从 `"true"` / `"false"` 解析

## 限制

环境变量最适合标量值（字符串、数字、布尔值）。复杂结构如 `authorized_clients` 数组和 `port_forwards` 数组建议通过 TOML 文件配置。
