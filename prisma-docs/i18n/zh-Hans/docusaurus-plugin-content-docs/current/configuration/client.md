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
| `transport` | string | `"quic"` | `quic` / `tcp` / `ws` / `grpc` / `xhttp` / `xporta` / `prisma-tls` |
| `skip_cert_verify` | bool | `false` | 跳过 TLS 证书验证 |
| `tls_on_tcp` | bool | `false` | 通过 TLS 包裹的 TCP 连接（须与服务端伪装设置匹配） |
| `tls_server_name` | string? | — | TLS SNI 服务器名称覆盖（默认使用 server_addr 的主机名） |
| `alpn_protocols` | string[] | `["h2", "http/1.1"]` | TLS/QUIC ALPN 协议 |
| `port_forwards[].name` | string | — | 端口转发的标签名称 |
| `port_forwards[].local_addr` | string | — | 本地服务地址（如 `127.0.0.1:3000`） |
| `port_forwards[].remote_port` | u16 | — | 在服务器端监听的端口 |
| `logging.level` | string | `"info"` | `trace` / `debug` / `info` / `warn` / `error` |
| `logging.format` | string | `"pretty"` | `pretty` / `json` |
| `ws_url` | string? | — | WebSocket 服务器 URL（如 `wss://domain.com/ws-tunnel`） |
| `ws_host` | string? | — | 覆盖 WebSocket `Host` 请求头 |
| `ws_extra_headers` | \[\[k,v\]\] | `[]` | 额外的 WebSocket 请求头 |
| `grpc_url` | string? | — | gRPC 服务器 URL |
| `xhttp_mode` | string? | — | XHTTP 模式：`"packet-up"` / `"stream-up"` / `"stream-one"` |
| `xhttp_upload_url` | string? | — | XHTTP packet-up/stream-up 上传 URL |
| `xhttp_download_url` | string? | — | XHTTP packet-up 下载 URL |
| `xhttp_stream_url` | string? | — | XHTTP stream-one 流 URL |
| `xhttp_extra_headers` | \[\[k,v\]\] | `[]` | 额外的 XHTTP 请求头 |
| `xporta.base_url` | string? | — | XPorta 服务器基础 URL（如 `https://your-domain.com`） |
| `xporta.session_path` | string | `"/api/auth"` | XPorta 会话初始化端点 |
| `xporta.data_paths` | string[] | `["/api/v1/data", ...]` | XPorta 上传端点路径 |
| `xporta.poll_paths` | string[] | `["/api/v1/notifications", ...]` | XPorta 长轮询下载路径 |
| `xporta.encoding` | string | `"json"` | XPorta 编码方式：`"json"` / `"binary"` / `"auto"` |
| `xporta.poll_concurrency` | u8 | `3` | 并发待处理轮询请求数（1-8） |
| `xporta.upload_concurrency` | u8 | `4` | 并发上传请求数（1-8） |
| `xporta.max_payload_size` | u32 | `65536` | 每请求最大负载字节数 |
| `xporta.poll_timeout_secs` | u16 | `55` | 长轮询超时时间（10-90 秒） |
| `xporta.extra_headers` | \[\[k,v\]\] | `[]` | 额外的 XPorta 请求头 |
| `xporta.cookie_name` | string | `"_sess"` | 会话 Cookie 名称（须与服务端配置匹配） |
| `xmux.max_connections_min` | u16 | `1` | 连接池最小连接数 |
| `xmux.max_connections_max` | u16 | `4` | 连接池最大连接数 |
| `xmux.max_concurrency_min` | u16 | `8` | 每连接最小并发数 |
| `xmux.max_concurrency_max` | u16 | `16` | 每连接最大并发数 |
| `xmux.max_lifetime_secs_min` | u64 | `300` | 连接最小生存时间（秒） |
| `xmux.max_lifetime_secs_max` | u64 | `600` | 连接最大生存时间（秒） |
| `xmux.max_requests_min` | u32 | `100` | 轮换前最小请求数 |
| `xmux.max_requests_max` | u32 | `200` | 轮换前最大请求数 |
| `user_agent` | string? | — | 覆盖 User-Agent 请求头 |
| `referer` | string? | — | 覆盖 Referer 请求头 |
| `congestion.mode` | string | `"bbr"` | 拥塞控制：`"brutal"` / `"bbr"` / `"adaptive"` |
| `congestion.target_bandwidth` | string? | — | brutal/adaptive 模式的目标带宽（如 `"100mbps"`） |
| `port_hopping.enabled` | bool | `false` | 启用 QUIC 端口跳变 |
| `port_hopping.base_port` | u16 | `10000` | 端口范围起始值 |
| `port_hopping.port_range` | u16 | `50000` | 端口范围数量 |
| `port_hopping.interval_secs` | u64 | `60` | 端口跳变间隔（秒） |
| `port_hopping.grace_period_secs` | u64 | `10` | 双端口接受窗口（秒） |
| `salamander_password` | string? | — | Salamander UDP 混淆密码（仅 QUIC） |
| `udp_fec.enabled` | bool | `false` | 启用 UDP 中继的前向纠错 |
| `udp_fec.data_shards` | usize | `10` | 每 FEC 组的原始数据包数 |
| `udp_fec.parity_shards` | usize | `3` | 每 FEC 组的校验包数 |
| `dns.mode` | string | `"direct"` | DNS 模式：`"smart"` / `"fake"` / `"tunnel"` / `"direct"` |
| `dns.fake_ip_range` | string | `"198.18.0.0/15"` | 虚假 DNS IP 的 CIDR 范围 |
| `dns.upstream` | string | `"8.8.8.8:53"` | 上游 DNS 服务器 |
| `dns.geosite_path` | string? | — | 智能 DNS 模式的 GeoSite 数据库路径 |
| `dns.dns_listen_addr` | string | `"127.0.0.1:53"` | 本地 DNS 服务器监听地址 |
| `routing.rules[].type` | string | — | 规则类型：`domain` / `domain-suffix` / `domain-keyword` / `ip-cidr` / `geoip` / `port` / `all` |
| `routing.rules[].value` | string | — | 匹配值（`geoip` 类型使用国家代码，如 `"cn"`、`"private"`） |
| `routing.rules[].action` | string | `"proxy"` | 动作：`"proxy"` / `"direct"` / `"block"` |
| `routing.geoip_path` | string? | — | v2fly geoip.dat 文件路径，用于 GeoIP 路由 |
| `tun.enabled` | bool | `false` | 启用 TUN 模式（系统级代理） |
| `tun.device_name` | string | `"prisma-tun0"` | TUN 设备名称 |
| `tun.mtu` | u16 | `1500` | TUN 设备 MTU |
| `tun.include_routes` | string[] | `["0.0.0.0/0"]` | TUN 模式捕获的路由 |
| `tun.exclude_routes` | string[] | `[]` | 排除的路由（服务器 IP 自动排除） |
| `tun.dns` | string | `"fake"` | TUN DNS 模式：`"fake"` / `"tunnel"` |
| `protocol_version` | string | `"v4"` | 协议版本（仅 v4） |
| `fingerprint` | string | `"chrome"` | uTLS 指纹：`chrome` / `firefox` / `safari` / `random` / `none` |
| `quic_version` | string | `"auto"` | QUIC 版本：`v2` / `v1` / `auto` |
| `transport_mode` | string | `"auto"` | 传输模式：`auto` 或显式名称 |
| `fallback_order` | string[] | `["quic-v2", ...]` | 自动模式的传输回退顺序 |
| `prisma_auth_secret` | string? | — | PrismaTLS 认证密钥（十六进制编码，须与服务端匹配） |
| `traffic_shaping.padding_mode` | string | `"none"` | `none` / `random` / `bucket` |
| `traffic_shaping.bucket_sizes` | u16[] | `[128,256,...]` | bucket 填充模式的桶大小 |
| `traffic_shaping.timing_jitter_ms` | u32 | `0` | 握手帧的最大时序抖动（毫秒） |
| `traffic_shaping.chaff_interval_ms` | u32 | `0` | 混淆注入间隔（毫秒），0=禁用 |
| `traffic_shaping.coalesce_window_ms` | u32 | `0` | 帧合并窗口（毫秒） |
| `sni_slicing` | bool | `false` | QUIC SNI 分片（将 ClientHello 分片到多个 CRYPTO 帧中） |
| `entropy_camouflage` | bool | `false` | Salamander/原始 UDP 的熵伪装 |
| `transport_only_cipher` | bool | `false` | 使用仅传输层加密模式（BLAKE3 MAC，无应用层加密）。仅当传输层已提供加密（TLS/QUIC）时安全。服务端也须启用。 |

## 完整示例

```toml title="client.toml"
socks5_listen_addr = "127.0.0.1:1080"
http_listen_addr = "127.0.0.1:8080"  # 可选，删除此行以禁用 HTTP 代理
server_addr = "127.0.0.1:8443"
cipher_suite = "chacha20-poly1305"   # 或 "aes-256-gcm"
transport = "quic"                   # 或 "tcp" / "ws" / "grpc" / "xhttp" / "xporta" / "prisma-tls"
skip_cert_verify = true              # 开发环境中使用自签名证书时设为 true

# v4 功能
protocol_version = "v4"
fingerprint = "chrome"        # uTLS 指纹，模拟浏览器 ClientHello
quic_version = "auto"         # "v2"、"v1" 或 "auto"
# prisma_auth_secret = "hex-encoded-32-bytes"   # PrismaTLS 传输使用

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
- `transport` 必须是以下之一：`quic`、`tcp`、`ws`、`grpc`、`xhttp`、`xporta`、`prisma-tls`
- `xhttp_mode`（当 transport 为 `xhttp` 时）必须是以下之一：`packet-up`、`stream-up`、`stream-one`
- `xhttp_mode = "stream-one"` 需要设置 `xhttp_stream_url`
- `xhttp_mode = "packet-up"` 或 `"stream-up"` 需要设置 `xhttp_upload_url` 和 `xhttp_download_url`
- XMUX 范围须满足 min ≤ max
- `transport = "xporta"` 时需要设置 `xporta.base_url`
- XPorta：所有路径必须以 `/` 开头
- XPorta：`data_paths` 和 `poll_paths` 不能为空且不能重叠
- XPorta：`encoding` 必须是以下之一：`json`、`binary`、`auto`
- XPorta：`poll_concurrency` 须为 1-8，`upload_concurrency` 须为 1-8
- XPorta：`poll_timeout_secs` 须为 10-90
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

### PrismaTLS（主动探测防御）

PrismaTLS 替代 REALITY，在直连场景下提供最强的主动探测防御。服务器对主动探测者来说与真实网站无法区分。

```toml
transport = "prisma-tls"
tls_server_name = "www.microsoft.com"
fingerprint = "chrome"
prisma_auth_secret = "hex-encoded-32-bytes"
```

详见 [PrismaTLS](/docs/features/prisma-tls) 了解详细配置。

### XPorta（最高隐蔽性 — CDN）

新一代 CDN 传输，将代理数据分片为多个短命的 REST API 风格请求。流量与普通 SPA 发起的 API 调用无法区分。

```toml
transport = "xporta"

[xporta]
base_url = "https://your-domain.com"
session_path = "/api/auth"
data_paths = ["/api/v1/data", "/api/v1/sync", "/api/v1/update"]
poll_paths = ["/api/v1/notifications", "/api/v1/feed", "/api/v1/events"]
encoding = "json"
```

详见 [XPorta 传输](/docs/features/xporta-transport) 了解详细配置。

## 禁用 HTTP 代理

HTTP CONNECT 代理是可选的。要禁用它，只需在配置中省略 `http_listen_addr` 字段：

```toml
socks5_listen_addr = "127.0.0.1:1080"
# http_listen_addr 未设置 — HTTP 代理已禁用
server_addr = "1.2.3.4:8443"
```

## 证书验证

在使用有效 TLS 证书的生产部署中，请保持 `skip_cert_verify` 为 `false`（默认值）。仅在开发环境中使用自签名证书时将其设为 `true`。
