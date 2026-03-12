---
sidebar_position: 1
---

# PrismaVeil 协议

PrismaVeil 是 Prisma 客户端和服务端之间使用的自定义线路协议。它提供认证密钥交换、加密数据传输和多路复用流管理。

## 握手

PrismaVeil 握手是一个四步过程，用于建立经过认证的加密会话：

```
客户端                                    服务端
  │                                         │
  │──── ClientHello ──────────────────────▶│  步骤 1
  │     (version, X25519 pubkey,            │
  │      timestamp, padding)                │
  │                                         │
  │◀──── ServerHello ─────────────────────│  步骤 2
  │     (X25519 pubkey,                     │
  │      encrypted challenge, padding)      │
  │                                         │
  │  双方：ECDH → BLAKE3 KDF → 会话密钥
  │                                         │
  │──── ClientAuth（已加密）──────────────▶│  步骤 3
  │     (client_id, HMAC-SHA256 token,      │
  │      cipher suite, challenge response)  │
  │                                         │
  │◀──── ServerAccept（已加密）───────────│  步骤 4
  │     (status, session_id)                │
  │                                         │
  │════ 加密数据帧 ═══════════════════════│
```

### 步骤 1：ClientHello

客户端生成临时 X25519 密钥对，并发送其公钥、协议版本和当前时间戳。

**线路格式：**

| 字段 | 大小 | 描述 |
|------|------|------|
| `version` | 1 字节 | 协议版本（`0x01`） |
| `client_ephemeral_pub` | 32 字节 | X25519 临时公钥 |
| `timestamp` | 8 字节（大端） | Unix 时间戳（秒） |
| `padding` | 可变 | 随机填充（0-256 字节） |

### 步骤 2：ServerHello

服务端生成自己的临时 X25519 密钥对，通过 ECDH 计算共享密钥，通过 BLAKE3 派生会话密钥，生成 32 字节随机挑战值，使用会话密钥加密后发送其公钥和加密的挑战值。

**线路格式：**

| 字段 | 大小 | 描述 |
|------|------|------|
| `server_ephemeral_pub` | 32 字节 | X25519 临时公钥 |
| `challenge_len` | 2 字节（大端） | 加密挑战值的长度 |
| `encrypted_challenge` | 可变 | 使用会话密钥加密的挑战值 |
| `padding` | 可变 | 随机填充（0-256 字节） |

### 步骤 3：ClientAuth（已加密）

客户端同样计算共享密钥并派生相同的会话密钥。解密挑战值，计算挑战值的 BLAKE3 哈希作为响应，生成 HMAC-SHA256 认证令牌，所有内容使用会话密钥加密后发送。

**明文格式（加密前）：**

| 字段 | 大小 | 描述 |
|------|------|------|
| `client_id` | 16 字节 | 客户端 UUID |
| `auth_token` | 32 字节 | HMAC-SHA256(auth_secret, client_id \|\| timestamp) |
| `cipher_suite` | 1 字节 | `0x01` = ChaCha20-Poly1305，`0x02` = AES-256-GCM |
| `challenge_response` | 32 字节 | 解密后挑战值的 BLAKE3 哈希 |

### 步骤 4：ServerAccept（已加密）

服务端验证挑战响应和认证令牌。如果有效，发送加密的接受消息，包含新的会话 UUID。

**明文格式（加密前）：**

| 字段 | 大小 | 描述 |
|------|------|------|
| `status` | 1 字节 | 接受状态码 |
| `session_id` | 16 字节 | 此会话的 UUID |

**AcceptStatus 值：**

| 代码 | 名称 | 描述 |
|------|------|------|
| `0x00` | Ok | 认证成功 |
| `0x01` | AuthFailed | 凭证无效 |
| `0x02` | ServerBusy | 已达最大连接数 |
| `0x03` | VersionMismatch | 不支持的协议版本 |

## 加密帧线路格式

握手完成后，所有数据以加密帧的形式交换：

```
[nonce:12 字节][密文长度:2 字节 大端][密文 + AEAD 标签]
```

nonce 随每个帧一起传输。密文包含 AEAD 认证标签（ChaCha20-Poly1305 和 AES-256-GCM 均为 16 字节）。

## 数据帧明文格式

每个加密帧内部的明文是一个数据帧：

```
[command:1][flags:1][stream_id:4][payload:可变]
```

| 字段 | 大小 | 描述 |
|------|------|------|
| `command` | 1 字节 | 命令类型（见下文） |
| `flags` | 1 字节 | 位标志（见下文） |
| `stream_id` | 4 字节（大端） | 流多路复用标识符 |
| `payload` | 可变 | 命令特定数据 |

## 命令类型

| 代码 | 命令 | 方向 | 载荷 |
|------|------|------|------|
| `0x01` | CONNECT | 客户端 → 服务端 | 目标地址 + 端口 |
| `0x02` | DATA | 双向 | 原始数据字节 |
| `0x03` | CLOSE | 双向 | 无 |
| `0x04` | PING | 双向 | 序列号（4 字节） |
| `0x05` | PONG | 双向 | 序列号（4 字节） |
| `0x06` | REGISTER_FORWARD | 客户端 → 服务端 | 远程端口（2 字节）+ 名称 |
| `0x07` | FORWARD_READY | 服务端 → 客户端 | 远程端口（2 字节）+ 成功标志（1 字节） |
| `0x08` | FORWARD_CONNECT | 服务端 → 客户端 | 远程端口（2 字节） |

## 标志位

| 位 | 名称 | 描述 |
|----|------|------|
| `0x01` | PADDED | 帧包含填充 |

## 协议常量

| 常量 | 值 | 描述 |
|------|-----|------|
| `PROTOCOL_VERSION` | `0x01` | 当前协议版本 |
| `MAX_FRAME_SIZE` | 16384 | 最大帧大小（字节） |
| `NONCE_SIZE` | 12 | Nonce 大小（字节） |
| `MAX_PADDING_SIZE` | 256 | 最大填充大小（字节） |
