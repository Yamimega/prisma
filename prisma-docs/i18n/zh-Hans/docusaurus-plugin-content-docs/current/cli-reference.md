---
sidebar_position: 6
---

# CLI 参考

`prisma` 二进制文件提供多个子命令，用于运行服务端和客户端、生成凭证、管理配置、启动控制面板，以及通过管理 API 控制运行中的服务器。

## 全局参数

以下参数适用于所有子命令：

| 参数 | 环境变量 | 描述 |
|------|----------|------|
| `--json` | — | 输出原始 JSON 而非格式化表格 |
| `--mgmt-url <URL>` | `PRISMA_MGMT_URL` | 管理 API 地址（覆盖自动检测） |
| `--mgmt-token <TOKEN>` | `PRISMA_MGMT_TOKEN` | 管理 API 认证令牌（覆盖自动检测） |

## `prisma server`

启动代理服务端。

```bash
prisma server -c <PATH>
```

| 参数 | 默认值 | 描述 |
|------|--------|------|
| `-c, --config <PATH>` | `server.toml` | 服务端配置文件路径 |

服务端同时启动 TCP 和 QUIC 监听器，等待客户端连接。启动时会验证配置，如果验证失败则退出并报错。

## `prisma client`

启动代理客户端。

```bash
prisma client -c <PATH>
```

| 参数 | 默认值 | 描述 |
|------|--------|------|
| `-c, --config <PATH>` | `client.toml` | 客户端配置文件路径 |

客户端启动 SOCKS5 监听器（以及可选的 HTTP CONNECT 监听器），连接到远程服务器，执行 PrismaVeil 握手，然后开始代理流量。

## `prisma gen-key`

生成新的客户端身份标识（UUID + 认证密钥对）。

```bash
prisma gen-key
```

无需参数。输出一个新的 UUID 和 64 字符的十六进制密钥，以及可直接粘贴到服务端和客户端配置文件的 TOML 代码片段：

```
Client ID:   a1b2c3d4-e5f6-7890-abcd-ef1234567890
Auth Secret: 4f8a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a

# 添加到 server.toml：
[[authorized_clients]]
id = "a1b2c3d4-e5f6-7890-abcd-ef1234567890"
auth_secret = "4f8a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a"
name = "my-client"

# 添加到 client.toml：
[identity]
client_id = "a1b2c3d4-e5f6-7890-abcd-ef1234567890"
auth_secret = "4f8a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a"
```

## `prisma gen-cert`

生成用于开发环境的自签名 TLS 证书。

```bash
prisma gen-cert -o <DIR> --cn <NAME>
```

| 参数 | 默认值 | 描述 |
|------|--------|------|
| `-o, --output <DIR>` | `.` | 证书和密钥文件的输出目录 |
| `--cn <NAME>` | `prisma-server` | 证书的通用名称（Common Name） |

在输出目录生成两个文件：

- `prisma-cert.pem` — 自签名 X.509 证书
- `prisma-key.pem` — PEM 格式的私钥

示例：

```bash
prisma gen-cert -o /etc/prisma --cn my-server.example.com
```

:::warning
自签名证书仅适用于开发环境。生产环境请使用受信任 CA 或 Let's Encrypt 颁发的证书。使用自签名证书时，客户端必须设置 `skip_cert_verify = true`。
:::

## `prisma init`

生成带注释的配置文件，并自动生成密钥。

```bash
prisma init [OPTIONS]
```

| 参数 | 默认值 | 描述 |
|------|--------|------|
| `--cdn` | — | 包含预配置的 CDN 部分 |
| `--server-only` | — | 仅生成服务端配置 |
| `--client-only` | — | 仅生成客户端配置 |
| `--force` | — | 覆盖已有文件 |

默认同时生成 `server.toml` 和 `client.toml`，包含新生成的 UUID、认证密钥和详细注释。使用 `--cdn` 可包含完整注释的 CDN 传输配置部分。

示例：

```bash
# 生成包含 CDN 部分的两个配置文件
prisma init --cdn

# 仅生成客户端配置，覆盖已有文件
prisma init --client-only --force
```

## `prisma validate`

在不启动服务的情况下验证配置文件。

```bash
prisma validate -c <PATH> [-t <TYPE>]
```

| 参数 | 默认值 | 描述 |
|------|--------|------|
| `-c, --config <PATH>` | — | 配置文件路径 |
| `-t, --type <TYPE>` | `server` | 配置类型：`server` 或 `client` |

解析 TOML 文件并运行所有验证规则。验证通过则以代码 0 退出，否则输出错误信息并以非零代码退出。

示例：

```bash
prisma validate -c server.toml
prisma validate -c client.toml -t client
```

## `prisma status`

查询管理 API 获取服务器状态。

```bash
prisma status
```

无命令专属参数。使用全局 `--mgmt-url` 和 `--mgmt-token` 参数（或对应的 `PRISMA_MGMT_URL` / `PRISMA_MGMT_TOKEN` 环境变量）。

连接到管理 API 并显示服务器健康状态、运行时间、版本和活跃连接数。

示例：

```bash
prisma status --mgmt-url https://127.0.0.1:9090 --mgmt-token your-auth-token
```

## `prisma speed-test`

运行针对服务器的带宽测试。

```bash
prisma speed-test -s <SERVER> [OPTIONS]
```

| 参数 | 默认值 | 描述 |
|------|--------|------|
| `-s, --server <HOST:PORT>` | — | 服务器地址 |
| `-d, --duration <SECS>` | `10` | 测试持续时间（秒） |
| `--direction <DIR>` | `both` | 方向：`download`、`upload` 或 `both` |
| `-C, --config <PATH>` | `client.toml` | 客户端配置文件（用于认证凭证） |

使用客户端配置进行认证并建立隧道，然后在指定方向上测量吞吐量。

示例：

```bash
prisma speed-test -s my-server.example.com:8443 -d 15 --direction download
```

## `prisma dashboard`

启动 Web 控制面板，支持自动下载和反向代理。

```bash
prisma dashboard [OPTIONS]
```

| 参数 | 默认值 | 描述 |
|------|--------|------|
| `--mgmt-url <URL>` | `https://127.0.0.1:9090` | 代理请求的管理 API 地址 |
| `--token <TOKEN>` | — | 管理 API 认证令牌 |
| `--port <PORT>` | `9091` | 控制面板服务端口 |
| `--bind <ADDR>` | `0.0.0.0` | 控制面板绑定地址 |
| `--no-open` | — | 不自动打开浏览器 |
| `--update` | — | 强制重新下载控制面板资源 |
| `--dir <PATH>` | — | 从本地目录提供控制面板，而非自动下载 |

首次运行时从 GitHub Releases 下载最新控制面板并缓存到本地。启动本地服务器提供静态文件并将 `/api/*` 请求反向代理到管理 API。

桌面系统会自动打开浏览器。无头/VPS 环境（SSH 会话、无 `$DISPLAY`）则打印 URL。

示例：

```bash
# 基本用法（连接本地管理 API）
prisma dashboard --token your-secure-token

# 连接远程服务器
prisma dashboard --mgmt-url https://my-server.com:9090 --token my-token

# 强制重新下载最新控制面板
prisma dashboard --update --token your-secure-token
```

## `prisma version`

显示版本信息、协议版本和支持的功能。

```bash
prisma version
```

无需参数。输出 Prisma 版本、PrismaVeil 协议版本、支持的加密算法、支持的传输方式和功能列表。

## `prisma completions`

生成 Shell 自动补全脚本。

```bash
prisma completions <SHELL>
```

| 参数 | 描述 |
|------|------|
| `<SHELL>` | 目标 Shell：`bash`、`fish`、`zsh`、`elvish`、`powershell` |

示例：

```bash
# Bash
prisma completions bash >> ~/.bash_completion

# Zsh
prisma completions zsh > ~/.zfunc/_prisma
```

---

## 管理 API 命令

以下命令通过管理 API 与运行中的服务器通信。根据需要设置 `--mgmt-url` 和 `--mgmt-token`（或对应的环境变量）。

## `prisma clients`

管理授权客户端。

```bash
prisma clients <SUBCOMMAND>
```

| 子命令 | 描述 |
|--------|------|
| `list` | 列出所有授权客户端 |
| `show <ID>` | 显示特定客户端的详情 |
| `create [--name NAME]` | 创建新客户端（自动生成密钥） |
| `delete <ID> [--yes]` | 删除客户端（`--yes` 跳过确认） |
| `enable <ID>` | 启用客户端 |
| `disable <ID>` | 禁用客户端 |

## `prisma connections`

管理活跃连接。

```bash
prisma connections <SUBCOMMAND>
```

| 子命令 | 描述 |
|--------|------|
| `list` | 列出活跃连接 |
| `disconnect <ID>` | 终止特定会话 |
| `watch [--interval N]` | 实时监控连接（默认间隔：2 秒） |

## `prisma metrics`

查看服务器指标和系统信息。

```bash
prisma metrics [OPTIONS]
```

| 参数 | 默认值 | 描述 |
|------|--------|------|
| `--watch` | — | 自动刷新指标 |
| `--history` | — | 显示历史指标 |
| `--period <PERIOD>` | `1h` | 历史周期：`1h`、`6h`、`24h`、`7d` |
| `--interval <SECS>` | `2` | 刷新间隔（秒，用于 `--watch`） |
| `--system` | — | 显示系统信息而非指标 |

## `prisma bandwidth`

管理每客户端带宽限制和流量配额。

```bash
prisma bandwidth <SUBCOMMAND>
```

| 子命令 | 描述 |
|--------|------|
| `summary` | 显示所有客户端的带宽概览 |
| `get <ID>` | 显示特定客户端的带宽和配额 |
| `set <ID> [--upload BPS] [--download BPS]` | 设置上传/下载限速（位/秒，0 = 不限速） |
| `quota <ID> [--limit BYTES]` | 获取或设置流量配额（字节） |

## `prisma config`

管理服务器配置。

```bash
prisma config <SUBCOMMAND>
```

| 子命令 | 描述 |
|--------|------|
| `get` | 显示当前服务器配置 |
| `set <KEY> <VALUE>` | 更新配置值（点分格式，如 `logging.level`） |
| `tls` | 显示 TLS 配置 |
| `backup create` | 创建配置备份 |
| `backup list` | 列出所有备份 |
| `backup restore <NAME>` | 恢复备份 |
| `backup diff <NAME>` | 显示备份与当前配置的差异 |
| `backup delete <NAME>` | 删除备份 |

## `prisma routes`

管理服务端路由规则。

```bash
prisma routes <SUBCOMMAND>
```

| 子命令 | 描述 |
|--------|------|
| `list` | 列出所有路由规则 |
| `create --name NAME --condition COND --action ACTION [--priority N]` | 创建路由规则 |
| `update <ID> [--condition COND] [--action ACTION] [--priority N] [--name NAME]` | 更新路由规则 |
| `delete <ID>` | 删除路由规则 |
| `setup <PRESET> [--clear]` | 应用预定义规则预设 |

条件格式：`TYPE:VALUE`，例如 `DomainMatch:*.ads.*`、`IpCidr:10.0.0.0/8`、`PortRange:80-443`、`All`。

### `prisma routes setup`

一键应用命名预设——批量创建一组精选规则。

```bash
prisma routes setup <PRESET> [--clear]
```

| 参数 | 描述 |
|------|------|
| `--clear` | 应用预设前删除所有已有规则 |

可用预设：

| 预设 | 规则数 | 描述 |
|------|--------|------|
| `block-ads` | 10 | 屏蔽常见广告和广告网络域名 |
| `privacy` | 19 | 屏蔽广告 + 分析/遥测追踪器 |
| `allow-all` | 1 | 添加全匹配 Allow 规则（优先级 1000） |
| `block-all` | 1 | 添加全匹配 Block 规则（优先级 1000） |

示例：

```bash
# 清空旧规则并应用广告屏蔽预设
prisma routes setup block-ads --clear

# 在现有规则基础上叠加隐私预设
prisma routes setup privacy

# 重置为单条 allow-all 规则
prisma routes setup allow-all --clear
```

## `prisma logs`

通过 WebSocket 实时流式传输服务器日志。

```bash
prisma logs [OPTIONS]
```

| 参数 | 默认值 | 描述 |
|------|--------|------|
| `--level <LEVEL>` | — | 最低日志级别：`TRACE`、`DEBUG`、`INFO`、`WARN`、`ERROR` |
| `--lines <N>` | — | 显示的最大日志行数 |

## `prisma ping`

测量到服务器的握手 RTT。

```bash
prisma ping [OPTIONS]
```

| 参数 | 默认值 | 描述 |
|------|--------|------|
| `-c, --config <PATH>` | `client.toml` | 客户端配置文件（用于认证凭证） |
| `-s, --server <HOST:PORT>` | — | 覆盖配置中的服务器地址 |
| `--count <N>` | `5` | 发送 ping 的次数 |
| `--interval <MS>` | `1000` | 两次 ping 之间的间隔（毫秒） |

## `prisma test-transport`

测试所有已配置的传输方式并报告哪些可用。

```bash
prisma test-transport [OPTIONS]
```

| 参数 | 默认值 | 描述 |
|------|--------|------|
| `-c, --config <PATH>` | `client.toml` | 客户端配置文件 |
