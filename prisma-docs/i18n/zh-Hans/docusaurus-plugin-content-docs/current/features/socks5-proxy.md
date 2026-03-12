---
sidebar_position: 1
---

# SOCKS5 代理

Prisma 在客户端实现了完整的 RFC 1928 SOCKS5 代理接口。应用程序连接到本地 SOCKS5 端口，流量通过加密的 PrismaVeil 连接透明地隧道传输到服务器。

## 支持的功能

- **IPv4** 地址连接
- **IPv6** 地址连接
- **域名** 解析（在服务端进行解析）
- **CONNECT** 命令（TCP 代理）
- 本地 SOCKS5 接口无需认证（通过本地绑定确保安全）

## 配置

SOCKS5 监听器始终启用。在 `client.toml` 中配置绑定地址：

```toml
socks5_listen_addr = "127.0.0.1:1080"
```

默认绑定到 `127.0.0.1:1080`。

## 数据流

```
应用程序 ──SOCKS5──▶ prisma-client ──PrismaVeil/QUIC──▶ prisma-server ──TCP──▶ 目标地址
```

1. 应用程序发送 SOCKS5 CONNECT 请求，包含目标地址和端口
2. 客户端解析 SOCKS5 请求，提取目标地址（IPv4、IPv6 或域名）
3. 客户端通过加密的 PrismaVeil 隧道发送 `CONNECT` 命令
4. 服务端解析目标地址（如果是域名）并建立 TCP 连接
5. 数据通过加密隧道双向中继

## 配合 curl 使用

```bash
curl --socks5 127.0.0.1:1080 https://httpbin.org/ip
curl --socks5-hostname 127.0.0.1:1080 https://example.com
```

`--socks5-hostname` 变体会将域名发送到服务端进行解析，避免本地 DNS 泄露。

## 配合浏览器使用

### Firefox

1. 打开 设置 > 网络设置 > 手动配置代理
2. 设置 SOCKS 主机为 `127.0.0.1`，端口为 `1080`
3. 选择 **SOCKS v5**
4. 勾选 **使用 SOCKS v5 时代理 DNS** 以防止 DNS 泄露

### Chrome / Chromium

使用代理参数启动：

```bash
chromium --proxy-server="socks5://127.0.0.1:1080"
```

## 系统全局代理

在 Linux 上，许多应用程序支持 `ALL_PROXY` 环境变量：

```bash
export ALL_PROXY=socks5://127.0.0.1:1080
```
