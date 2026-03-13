---
sidebar_position: 6
---

# XHTTP 传输

XHTTP 是受 Xray-core XHTTP 协议启发的 HTTP 原生传输模式。它将 PrismaVeil 帧封装在看起来正常的 HTTP 流量中，使连接对网络观察者和 CDN 基础设施而言与普通网页浏览无法区分。

## 传输模式

### packet-up

客户端将 PrismaVeil 帧作为单独的 HTTP POST 请求发送到上传端点。服务器通过长连接 GET 请求到下载端点（可选包装为 Server-Sent Events）回复数据。

```
客户端 ──POST /api/v1/upload──▶ 服务器（上传分块）
客户端 ◀──GET /api/v1/events─── 服务器（下载流）
```

适用于：具有激进 HTTP 超时或缓冲请求体的代理环境。

### stream-up

客户端发送带有分块请求体的流式 HTTP POST。服务器以流式响应体回复。需要 HTTP/1.1 分块传输或 HTTP/2。

```
客户端 ──POST（流式请求体）──▶ 服务器
客户端 ◀──（流式响应）─── 服务器
```

适用于：支持流式请求体的 CDN（例如 Cloudflare 的 gRPC 或 WebSocket 回退）。

### stream-one

单一 HTTP/2 双向流。请求体承载上传数据，响应体承载下载数据，都在一个连接上。

```
客户端 ═══ H2 POST /api/v1/stream ═══ 服务器
          （双向流式传输）
```

适用于：支持端到端 HTTP/2 的环境。最高效的模式。

## 服务端配置

启用 CDN 监听器并配置 XHTTP 路径：

```toml
[cdn]
enabled = true
listen_addr = "0.0.0.0:443"

# XHTTP 端点（以下为默认路径）
xhttp_upload_path = "/api/v1/upload"
xhttp_download_path = "/api/v1/events"
xhttp_stream_path = "/api/v1/stream"

# 可选：将下载包装为 SSE 格式以提高 CDN 兼容性
# xhttp_nosse = false

# 头部混淆
response_server_header = "nginx"    # 伪装为 nginx
padding_header = true               # 添加 X-Padding 响应头
# xhttp_extra_headers = [["X-Powered-By", "Express"]]

[cdn.tls]
cert_path = "origin-cert.pem"
key_path = "origin-key.pem"
```

## 客户端配置

```toml
transport = "xhttp"
xhttp_mode = "stream-one"
xhttp_stream_url = "https://your-domain.com/api/v1/stream"

# 对于 packet-up 或 stream-up 模式：
# xhttp_mode = "packet-up"
# xhttp_upload_url = "https://your-domain.com/api/v1/upload"
# xhttp_download_url = "https://your-domain.com/api/v1/events"

# 头部混淆
user_agent = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36"
referer = "https://www.google.com/"
# xhttp_extra_headers = [["Accept", "text/html"]]
```

## XMUX 连接池

XMUX 随机化连接生命周期以防止长连接指纹识别。连接池中的每个连接从可配置范围中获取随机的生命周期和请求数限制。

```toml
[xmux]
max_connections_min = 1       # 最小连接池大小
max_connections_max = 4       # 最大连接池大小
max_concurrency_min = 8       # 每个连接最小并发流数
max_concurrency_max = 16      # 每个连接最大并发流数
max_lifetime_secs_min = 300   # 最小连接生命周期（5 分钟）
max_lifetime_secs_max = 600   # 最大连接生命周期（10 分钟）
max_requests_min = 100        # 轮换前最小请求数
max_requests_max = 200        # 轮换前最大请求数
```

当连接超过其随机化的生命周期或请求数时，会被优雅关闭并替换为新连接。

## HTTP 头部混淆

客户端和服务端都可以注入头部，使流量看起来像正常的网页浏览：

### 服务端

- **`response_server_header`**：覆盖 `Server` 响应头（例如 `"nginx"`、`"cloudflare"`）
- **`padding_header`**：添加随机长度值的 `X-Padding` 响应头（抵御响应大小指纹识别）
- **`xhttp_extra_headers`**：添加任意响应头（例如 `X-Powered-By`、`X-Request-ID`）

### 客户端

- **`user_agent`**：覆盖 `User-Agent` 请求头
- **`referer`**：添加 `Referer` 请求头
- **`xhttp_extra_headers`**：添加任意请求头

## 与其他传输协议的比较

| 特性 | QUIC | TCP | WebSocket | gRPC | XHTTP | XPorta |
|------|------|-----|-----------|------|-------|--------|
| 看起来像正常 HTTP | 否 | 否 | Upgrade 头部 | grpc content-type | 是 | 是 |
| CDN 兼容 | 否 | 否 | 是 | 部分 | 是 | 是 |
| DPI 抗性 | 低 | 低 | 中 | 中 | 高 | 最高 |
| 抗主动探测 | 否 | 否 | 否 | 否 | 否 | 是 |
| 单连接开销 | 低 | 低 | 中 | 中 | 低（stream-one） | 低 |
| 多种上传模式 | 否 | 否 | 否 | 否 | 3 种模式 | 上传 + 长轮询 |
| SSE 伪装 | 不适用 | 不适用 | 不适用 | 不适用 | 是 | 不适用 |
| 头部混淆 | 不适用 | 不适用 | 基本 | 否 | 完整 | 完整 |

:::tip
如需最高级别的 DPI 抗性，请考虑使用 [XPorta 传输](/docs/features/xporta-transport) — 它将流量分片为短命的 REST API 风格请求，与正常 Web 应用流量无法区分。
:::
