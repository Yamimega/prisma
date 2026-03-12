---
sidebar_position: 9
---

# 故障排除

常见问题及其解决方案。

## 认证失败

**症状：** 客户端连接失败，显示 "Authentication failed" 或 `AcceptStatus::AuthFailed`。

**原因：**
- `client.toml` 中的 `client_id` 与 `server.toml` 中 `authorized_clients` 的任何条目都不匹配
- 客户端和服务端配置中的 `auth_secret` 不匹配
- `auth_secret` 不是有效的十六进制（必须恰好 64 个十六进制字符）

**解决方案：**

1. 重新运行 `prisma gen-key` 生成新的密钥对
2. 将输出复制到 `server.toml` 和 `client.toml`
3. 验证值完全匹配——没有多余的空格或截断

## TLS 证书错误

**症状：** 连接失败并出现 TLS 相关错误。

**原因：**
- 配置路径中的证书或密钥文件不存在
- 证书过期或无效
- 客户端使用 `skip_cert_verify = false` 连接到自签名证书的服务器

**解决方案：**

- 验证 `server.toml` 中的文件路径是否正确且文件存在
- 在开发环境中使用自签名证书时，在 `client.toml` 中设置 `skip_cert_verify = true`
- 生产环境请使用受信任 CA 颁发的证书
- 重新生成证书：`prisma gen-cert -o . --cn prisma-server`

## QUIC 连接失败（UDP 被阻断）

**症状：** 当 `transport = "quic"` 时客户端无法连接，但服务器通过 TCP 可达。

**原因：**
- 防火墙或网络阻断了服务器端口的 UDP 流量
- 某些网络（企业、酒店）完全阻断 UDP

**解决方案：**

在 `client.toml` 中切换到 TCP 传输：

```toml
transport = "tcp"
```

TCP 传输使用 TLS over TCP，提供相同的加密保证。

## 端口转发被拒绝

**症状：** 客户端日志显示 `ForwardReady` 且 `success = false`。

**原因：**
- 服务端未启用端口转发（`enabled = false` 或缺失）
- 请求的 `remote_port` 超出服务端允许的范围
- 该端口在服务端已被占用

**解决方案：**

1. 验证服务端已启用端口转发：

```toml
[port_forwarding]
enabled = true
port_range_start = 10000
port_range_end = 20000
```

2. 确保客户端的 `remote_port` 值在服务端配置的范围内
3. 检查该端口在服务端是否已被其他进程占用

## 连接超时

**症状：** 连接在一段时间不活动后被断开。

**原因：** `connection_timeout_secs` 设置对于空闲连接来说过低。

**解决方案：**

在 `server.toml` 中增加超时时间：

```toml
[performance]
connection_timeout_secs = 600  # 10 分钟
```

## 调试日志

启用 debug 或 trace 日志以诊断问题：

```toml
[logging]
level = "debug"   # 或 "trace" 以获取最详细的输出
format = "pretty"
```

或通过环境变量覆盖，无需修改配置文件：

```bash
PRISMA_LOGGING_LEVEL=trace prisma server -c server.toml
```

调试日志中需要关注的关键内容：

- 握手步骤完成消息
- 连接建立和断开事件
- 端口转发注册结果
- 加密/解密错误
