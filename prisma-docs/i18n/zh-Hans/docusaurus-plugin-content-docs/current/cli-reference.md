---
sidebar_position: 6
---

# CLI 参考

`prisma` 二进制文件提供四个子命令，用于运行服务端、客户端以及生成凭证。

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
