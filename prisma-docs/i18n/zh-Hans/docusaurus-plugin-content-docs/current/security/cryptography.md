---
sidebar_position: 2
---

# 密码学

Prisma 使用现代、经过审计的密码学原语来执行所有安全关键操作。本页记录了密码学组件及其作用。

## 概览

| 组件 | 算法 | 用途 |
|------|------|------|
| 密钥交换 | X25519 ECDH | 每会话的临时共享密钥 |
| 密钥派生 | BLAKE3 `derive_key` | 从共享密钥 + 上下文生成会话密钥 |
| 数据加密 | ChaCha20-Poly1305 或 AES-256-GCM | 数据帧的认证加密 |
| 身份认证 | HMAC-SHA256 | 客户端身份验证 |
| 挑战-响应 | BLAKE3 哈希 | 证明客户端派生了正确的会话密钥 |
| Nonce 构造 | 基于计数器 | 按方向的单调递增 nonce |
| 抗重放 | 1024 位滑动位图 | 检测重放或乱序帧 |

## X25519 ECDH 密钥交换

每个会话以临时 X25519 Diffie-Hellman 密钥交换开始。客户端和服务端都使用操作系统随机源（`OsRng`）生成新的密钥对，确保前向保密——即使长期密钥被泄露也不会暴露过去的会话密钥。

共享密钥的计算方式为：

```
shared_secret = X25519(my_secret, their_public)
```

双方得到相同的 32 字节共享密钥。

## BLAKE3 密钥派生

会话密钥使用 BLAKE3 的密钥派生模式和域分离字符串从共享密钥派生：

- **域字符串：** `"prisma-veil-v1-session-key"`
- **上下文（104 字节）：** `shared_secret (32) || client_pub (32) || server_pub (32) || timestamp (8)`

```
session_key = BLAKE3_derive_key(
    domain = "prisma-veil-v1-session-key",
    context = shared_secret || client_pub || server_pub || timestamp
)
```

在上下文中包含双方公钥和时间戳可确保即使在临时密钥被重用的极端情况下，会话密钥也是唯一的。

## AEAD 加密

所有数据帧使用客户端在握手期间选择的 AEAD 密码进行加密：

### ChaCha20-Poly1305（默认）

- 256 位密钥，96 位 nonce，128 位认证标签
- 软件优化——在没有 AES 硬件加速的平台上性能优异
- 默认加密套件

### AES-256-GCM

- 256 位密钥，96 位 nonce，128 位认证标签
- 在支持 AES-NI 指令的平台上有硬件加速
- 备选加密套件

两种密码都提供带关联数据的认证加密（AEAD），确保每个帧的机密性和完整性。

## HMAC-SHA256 身份认证

客户端身份使用 HMAC-SHA256 验证。在握手期间，客户端计算：

```
auth_token = HMAC-SHA256(auth_secret, client_id || timestamp)
```

服务端使用其存储的认证密钥副本独立计算相同的令牌，并使用常量时间比较验证相等性。

## 挑战-响应

在握手期间，服务端发送加密的随机挑战值。客户端通过以下步骤证明它派生了正确的会话密钥：

1. 使用派生的会话密钥解密挑战值
2. 计算 `challenge_response = BLAKE3(challenge)`
3. 在加密的 `ClientAuth` 消息中发送响应

服务端验证响应是否与预期值匹配。

## Nonce 结构

Nonce 为 12 字节（96 位），结构如下：

```
[direction:1][reserved:3][counter:8]
```

| 字段 | 大小 | 描述 |
|------|------|------|
| `direction` | 1 字节 | `0x00` = 客户端→服务端，`0x01` = 服务端→客户端 |
| `reserved` | 3 字节 | 填零 |
| `counter` | 8 字节（大端） | 单调递增计数器 |

方向字节确保即使计数器值相同，客户端到服务端和服务端到客户端的 nonce 也始终不同。这防止了跨方向的 nonce 重用。

## 常量时间比较

所有安全敏感的比较（认证令牌、挑战响应）使用 `subtle` crate 提供的常量时间相等性检查。这可以防止可能泄露密钥信息的时序侧信道攻击。
