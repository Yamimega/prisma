---
sidebar_position: 2
---

# HTTP CONNECT 代理

Prisma 在客户端支持可选的 HTTP CONNECT 代理接口。这允许 HTTP 感知的应用程序和浏览器通过加密的 PrismaVeil 连接隧道传输 HTTPS 流量。

## 工作原理

HTTP CONNECT 方法通过代理建立 TCP 隧道：

1. 客户端发送 `CONNECT example.com:443 HTTP/1.1`
2. Prisma 响应 `HTTP/1.1 200 Connection Established`
3. 后续所有数据通过加密隧道透明中继

## 配置

HTTP CONNECT 代理是**可选的**。要启用它，在 `client.toml` 中添加 `http_listen_addr`：

```toml
http_listen_addr = "127.0.0.1:8080"
```

要禁用 HTTP 代理，只需省略此字段。

## 配合 curl 使用

```bash
curl --proxy http://127.0.0.1:8080 https://httpbin.org/ip
```

## 配合环境变量使用

许多命令行工具和应用程序支持标准代理环境变量：

```bash
export https_proxy=http://127.0.0.1:8080
export http_proxy=http://127.0.0.1:8080

# 现在这些工具会自动使用代理：
curl https://example.com
wget https://example.com
```

## 配合浏览器使用

### Firefox

1. 打开 设置 > 网络设置 > 手动配置代理
2. 设置 HTTP 代理为 `127.0.0.1`，端口为 `8080`
3. 勾选 **也将此代理用于 HTTPS**

### Chrome / Chromium

```bash
chromium --proxy-server="http://127.0.0.1:8080"
```

## SOCKS5 与 HTTP CONNECT 对比

| 功能 | SOCKS5 | HTTP CONNECT |
|------|--------|-------------|
| 协议支持 | 任意 TCP | HTTP/HTTPS |
| 应用程序支持 | 广泛（curl、浏览器、大多数应用） | HTTP 感知应用 |
| DNS 解析 | 服务端（使用 `--socks5-hostname`） | 服务端 |
| 配置 | 始终启用 | 可选 |

两种接口都通过相同的加密 PrismaVeil 连接隧道传输流量。根据您的应用程序的代理支持情况选择。
