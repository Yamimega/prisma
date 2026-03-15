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
| `management_api.enabled` | bool | `false` | 启用管理 REST/WS API |
| `management_api.listen_addr` | string | `"0.0.0.0:9090"` | 管理 API 绑定地址 |
| `management_api.auth_token` | string | — | API 认证的 Bearer 令牌 |
| `management_api.cors_origins` | string[] | `[]` | 允许的 CORS 来源（用于外部仪表盘开发） |
| `management_api.dashboard_dir` | string? | — | 已构建仪表盘静态文件路径 |
| `management_api.tls.cert_path` | string? | — | 管理 API 的 TLS 证书路径（未设置时继承 `[tls]` 配置） |
| `management_api.tls.key_path` | string? | — | 管理 API 的 TLS 私钥路径（未设置时继承 `[tls]` 配置） |
| `management_api.tls_enabled` | bool | `true` | 启用管理 API 的 TLS（自动继承服务器 `[tls]` 配置） |
| `padding.min` | u16 | `0` | 每帧最小填充字节数 |
| `padding.max` | u16 | `256` | 每帧最大填充字节数 |
| `camouflage.enabled` | bool | `false` | 启用伪装（抗主动探测） |
| `camouflage.tls_on_tcp` | bool | `false` | 在 TCP 传输外包裹 TLS（需要 `[tls]` 配置） |
| `camouflage.fallback_addr` | string? | — | 非 Prisma 连接的诱饵服务器地址 |
| `camouflage.alpn_protocols` | string[] | `["h2", "http/1.1"]` | TLS/QUIC ALPN 协议 |
| `camouflage.h3_cover_site` | string? | — | HTTP/3 伪装上游 URL（代理真实网站） |
| `camouflage.h3_static_dir` | string? | — | HTTP/3 伪装本地静态文件目录 |
| `camouflage.salamander_password` | string? | — | Salamander UDP 混淆密码（仅 QUIC） |
| `cdn.enabled` | bool | `false` | 启用 CDN 传输监听（WS、gRPC、XHTTP） |
| `cdn.listen_addr` | string | `"0.0.0.0:443"` | CDN 监听绑定地址 |
| `cdn.tls.cert_path` | string? | — | CDN TLS 证书（如 Cloudflare Origin 证书） |
| `cdn.tls.key_path` | string? | — | CDN TLS 私钥 |
| `cdn.ws_tunnel_path` | string | `"/ws-tunnel"` | WebSocket 隧道端点路径 |
| `cdn.grpc_tunnel_path` | string | `"/tunnel.PrismaTunnel"` | gRPC 隧道服务路径 |
| `cdn.cover_upstream` | string? | — | 伪装流量的反向代理上游 URL |
| `cdn.cover_static_dir` | string? | — | 伪装流量的静态文件目录 |
| `cdn.trusted_proxies` | string[] | `[]` | 受信任的代理 IP 范围（如 Cloudflare CIDR） |
| `cdn.expose_management_api` | bool | `false` | 通过 CDN 端点暴露管理 API |
| `cdn.management_api_path` | string | `"/prisma-mgmt"` | CDN 上的管理 API 子路径 |
| `cdn.xhttp_upload_path` | string | `"/api/v1/upload"` | XHTTP packet-up 上传端点 |
| `cdn.xhttp_download_path` | string | `"/api/v1/events"` | XHTTP packet-up 下载端点 |
| `cdn.xhttp_stream_path` | string | `"/api/v1/stream"` | XHTTP stream-one/stream-up 端点 |
| `cdn.xhttp_mode` | string? | — | XHTTP 模式：`"packet-up"` / `"stream-up"` / `"stream-one"` |
| `cdn.xhttp_nosse` | bool | `false` | 禁用 XHTTP 下载的 SSE 包装 |
| `cdn.response_server_header` | string? | — | 覆盖 HTTP `Server` 响应头 |
| `cdn.padding_header` | bool | `true` | 添加 `X-Padding` 响应头 |
| `cdn.enable_sse_disguise` | bool | `false` | 以 SSE 格式包装下载流 |
| `cdn.xhttp_extra_headers` | \[\[k,v\]\] | `[]` | 额外的伪装响应头 |
| `cdn.xporta.enabled` | bool | `false` | 启用 XPorta 传输 |
| `cdn.xporta.session_path` | string | `"/api/auth"` | XPorta 会话端点 |
| `cdn.xporta.data_paths` | string[] | `["/api/v1/data", ...]` | XPorta 上传路径 |
| `cdn.xporta.poll_paths` | string[] | `["/api/v1/notifications", ...]` | XPorta 长轮询下载路径 |
| `cdn.xporta.session_timeout_secs` | u64 | `300` | 会话空闲超时时间（秒） |
| `cdn.xporta.max_sessions_per_client` | u16 | `8` | 每客户端最大并发会话数 |
| `cdn.xporta.cookie_name` | string | `"_sess"` | 会话 Cookie 名称 |
| `cdn.xporta.encoding` | string | `"json"` | 编码方式：`"json"` / `"binary"` |
| `dns_upstream` | string | `"8.8.8.8:53"` | CMD_DNS_QUERY 转发的上游 DNS 服务器 |
| `congestion.mode` | string | `"bbr"` | 拥塞控制：`"brutal"` / `"bbr"` / `"adaptive"` |
| `congestion.target_bandwidth` | string? | — | brutal/adaptive 模式的目标带宽（如 `"100mbps"`） |
| `port_hopping.enabled` | bool | `false` | 启用 QUIC 端口跳变 |
| `port_hopping.base_port` | u16 | `10000` | 端口范围起始值 |
| `port_hopping.port_range` | u16 | `50000` | 端口范围数量 |
| `port_hopping.interval_secs` | u64 | `60` | 端口跳变间隔（秒） |
| `port_hopping.grace_period_secs` | u64 | `10` | 跳变后旧端口保留时间（秒） |
| `authorized_clients[].bandwidth_up` | string? | — | 单客户端上传速率限制（如 `"100mbps"`） |
| `authorized_clients[].bandwidth_down` | string? | — | 单客户端下载速率限制 |
| `authorized_clients[].quota` | string? | — | 单客户端流量配额（如 `"100GB"`） |
| `authorized_clients[].quota_period` | string? | — | 配额周期：`"daily"` / `"weekly"` / `"monthly"` |
| `protocol_version` | string | `"v4"` | 协议版本（仅 v4） |
| `prisma_tls.enabled` | bool | `false` | 启用 PrismaTLS（替代 REALITY） |
| `prisma_tls.mask_servers` | array | `[]` | 掩护服务器池 |
| `prisma_tls.mask_servers[].addr` | string | — | 掩护服务器地址（如 `"www.microsoft.com:443"`） |
| `prisma_tls.mask_servers[].names` | string[] | `[]` | 允许的 SNI 名称 |
| `prisma_tls.auth_secret` | string | `""` | PrismaTLS 认证密钥（十六进制编码，32 字节） |
| `prisma_tls.auth_rotation_hours` | u64 | `1` | 认证密钥轮换间隔（小时） |
| `traffic_shaping.padding_mode` | string | `"none"` | `none` / `random` / `bucket` |
| `traffic_shaping.bucket_sizes` | u16[] | `[128,256,...]` | 桶填充模式的桶大小 |
| `traffic_shaping.timing_jitter_ms` | u32 | `0` | 握手帧最大时序抖动（毫秒） |
| `traffic_shaping.chaff_interval_ms` | u32 | `0` | 杂音注入间隔（毫秒），0=禁用 |
| `traffic_shaping.coalesce_window_ms` | u32 | `0` | 帧合并窗口（毫秒），0=禁用 |
| `allow_transport_only_cipher` | bool | `false` | 允许客户端使用仅传输层加密模式（BLAKE3 MAC，无应用层加密）。仅当传输层已提供加密（TLS/QUIC）时安全。 |
| `anti_rtt.enabled` | bool | `false` | 启用 RTT 归一化 |
| `anti_rtt.normalization_ms` | u32 | `150` | RTT 归一化目标值 |
| `routing.rules[].type` | string | — | 规则类型：`domain` / `domain-suffix` / `domain-keyword` / `ip-cidr` / `geoip` / `port` / `all` |
| `routing.rules[].value` | string | — | 匹配值 |
| `routing.rules[].action` | string | — | 动作：`"allow"` / `"block"`（或 `"proxy"` / `"direct"` 映射为 allow） |
| `routing.geoip_path` | string? | — | v2fly geoip.dat 文件路径，用于 GeoIP 路由 |

## 完整示例

```toml title="server.toml"
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

# 管理 API + 仪表盘
[management_api]
enabled = true
listen_addr = "127.0.0.1:9090"
auth_token = "your-secure-token-here"
dashboard_dir = "/opt/prisma/dashboard"  # 已构建仪表盘静态文件路径

# 每帧填充
[padding]
min = 0
max = 256

# 伪装（抗主动探测）
[camouflage]
enabled = true
tls_on_tcp = true
fallback_addr = "example.com:443"
alpn_protocols = ["h2", "http/1.1"]
# salamander_password = "shared-obfuscation-key"  # Salamander UDP 混淆（QUIC）
# h3_cover_site = "https://example.com"           # HTTP/3 伪装覆盖站点
# h3_static_dir = "/var/www/html"                 # 或提供本地静态文件用于 H3 伪装

# PrismaTLS（替代 REALITY 的主动探测防御）
# [prisma_tls]
# enabled = true
# auth_secret = "hex-encoded-32-bytes"
# auth_rotation_hours = 1
# [[prisma_tls.mask_servers]]
# addr = "www.microsoft.com:443"
# names = ["www.microsoft.com"]
# [[prisma_tls.mask_servers]]
# addr = "www.apple.com:443"
# names = ["www.apple.com"]

# 流量整形（抗指纹识别）
# [traffic_shaping]
# padding_mode = "bucket"
# bucket_sizes = [128, 256, 512, 1024, 2048, 4096, 8192, 16384]
# timing_jitter_ms = 30
# chaff_interval_ms = 500
# coalesce_window_ms = 5

# CDN 传输（通过 Cloudflare 的 WebSocket + gRPC + XHTTP）
# [cdn]
# enabled = true
# listen_addr = "0.0.0.0:443"
# ws_tunnel_path = "/ws-tunnel"
# grpc_tunnel_path = "/tunnel.PrismaTunnel"
# cover_upstream = "http://127.0.0.1:3000"        # 反向代理到真实网站
# trusted_proxies = ["173.245.48.0/20"]            # Cloudflare IP 范围
# [cdn.tls]
# cert_path = "origin-cert.pem"
# key_path = "origin-key.pem"
#
# XPorta 传输（新一代 REST API 模拟）
# [cdn.xporta]
# enabled = true
# session_path = "/api/auth"
# data_paths = ["/api/v1/data", "/api/v1/sync", "/api/v1/update"]
# poll_paths = ["/api/v1/notifications", "/api/v1/feed", "/api/v1/events"]
# session_timeout_secs = 300
# cookie_name = "_sess"
# encoding = "json"

# 静态路由规则（重启后保持不变）
# [routing]
# geoip_path = "/etc/prisma/geoip.dat"
# [[routing.rules]]
# type = "ip-cidr"
# value = "10.0.0.0/8"
# action = "block"
# [[routing.rules]]
# type = "domain-keyword"
# value = "torrent"
# action = "block"
# [[routing.rules]]
# type = "all"
# action = "allow"
```

## 验证规则

服务端配置在启动时进行验证，以下规则将被强制执行：

- `listen_addr` 不能为空
- `authorized_clients` 中至少需要一个条目
- 每个 `authorized_clients[].id` 不能为空
- 每个 `authorized_clients[].auth_secret` 不能为空且必须是有效的十六进制字符串
- `logging.level` 必须是以下之一：`trace`、`debug`、`info`、`warn`、`error`
- `logging.format` 必须是以下之一：`pretty`、`json`
- `camouflage.tls_on_tcp = true` 需要设置 `tls.cert_path` 和 `tls.key_path`

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

客户端也可以通过[管理 API](/docs/features/management-api)或[仪表盘](/docs/features/dashboard)在运行时管理，无需重启服务器。

## 管理 API 配置

管理 API 默认禁用。启用后，它会启动一个 HTTP 服务器（axum），同时提供 REST 端点和 WebSocket 连接。

:::warning
`auth_token` 保护所有管理 API 端点。生产环境请使用强随机令牌。
:::

**绑定地址**：默认 API 监听 `127.0.0.1:9090`（仅本地）。要暴露到网络，请更改 `listen_addr`——但请确保有适当的网络级别访问控制。

**仪表盘**：将 `dashboard_dir` 设置为包含已构建仪表盘静态文件的路径。服务器将在管理 API 地址提供仪表盘服务。从[最新版本](https://github.com/Yamimega/prisma/releases/latest)下载预构建文件，或使用 `cd prisma-dashboard && npm ci && npm run build` 从源码构建。

**CORS 来源**：仅在仪表盘开发服务器运行在不同来源时需要（如 `http://localhost:3000`）。生产环境中仪表盘由服务器自身提供时不需要。
