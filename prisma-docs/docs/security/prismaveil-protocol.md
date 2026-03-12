---
sidebar_position: 1
---

# PrismaVeil Protocol

PrismaVeil is the custom wire protocol used between the Prisma client and server. It provides authenticated key exchange, encrypted data transport, and multiplexed stream management.

## Handshake

The PrismaVeil handshake is a 4-step process that establishes an authenticated, encrypted session:

```
Client                                    Server
  │                                         │
  │──── ClientHello ──────────────────────▶│  Step 1
  │     (version, X25519 pubkey,            │
  │      timestamp, padding)                │
  │                                         │
  │◀──── ServerHello ─────────────────────│  Step 2
  │     (X25519 pubkey,                     │
  │      encrypted challenge, padding)      │
  │                                         │
  │  Both sides: ECDH → BLAKE3 KDF → session key
  │                                         │
  │──── ClientAuth (encrypted) ───────────▶│  Step 3
  │     (client_id, HMAC-SHA256 token,      │
  │      cipher suite, challenge response)  │
  │                                         │
  │◀──── ServerAccept (encrypted) ────────│  Step 4
  │     (status, session_id)                │
  │                                         │
  │════ Encrypted data frames ════════════│
```

### Step 1: ClientHello

The client generates an ephemeral X25519 key pair and sends its public key along with the protocol version and current timestamp.

**Wire format:**

| Field | Size | Description |
|-------|------|-------------|
| `version` | 1 byte | Protocol version (`0x01`) |
| `client_ephemeral_pub` | 32 bytes | X25519 ephemeral public key |
| `timestamp` | 8 bytes (BE) | Unix timestamp in seconds |
| `padding` | variable | Random padding (0-256 bytes) |

### Step 2: ServerHello

The server generates its own ephemeral X25519 key pair, computes the shared secret via ECDH, derives the session key via BLAKE3, generates a random 32-byte challenge, encrypts it with the session key, and sends its public key and the encrypted challenge.

**Wire format:**

| Field | Size | Description |
|-------|------|-------------|
| `server_ephemeral_pub` | 32 bytes | X25519 ephemeral public key |
| `challenge_len` | 2 bytes (BE) | Length of encrypted challenge |
| `encrypted_challenge` | variable | Challenge encrypted with session key |
| `padding` | variable | Random padding (0-256 bytes) |

### Step 3: ClientAuth (encrypted)

The client also computes the shared secret and derives the same session key. It decrypts the challenge, computes a BLAKE3 hash of the challenge as the response, generates an HMAC-SHA256 auth token, and sends all of this encrypted with the session key.

**Plaintext format (before encryption):**

| Field | Size | Description |
|-------|------|-------------|
| `client_id` | 16 bytes | Client UUID |
| `auth_token` | 32 bytes | HMAC-SHA256(auth_secret, client_id \|\| timestamp) |
| `cipher_suite` | 1 byte | `0x01` = ChaCha20-Poly1305, `0x02` = AES-256-GCM |
| `challenge_response` | 32 bytes | BLAKE3 hash of the decrypted challenge |

### Step 4: ServerAccept (encrypted)

The server verifies the challenge response and auth token. If valid, it sends an encrypted accept message with a new session UUID.

**Plaintext format (before encryption):**

| Field | Size | Description |
|-------|------|-------------|
| `status` | 1 byte | Accept status code |
| `session_id` | 16 bytes | UUID for this session |

**AcceptStatus values:**

| Code | Name | Description |
|------|------|-------------|
| `0x00` | Ok | Authentication successful |
| `0x01` | AuthFailed | Invalid credentials |
| `0x02` | ServerBusy | Max connections reached |
| `0x03` | VersionMismatch | Unsupported protocol version |

## Encrypted frame wire format

After the handshake, all data is exchanged as encrypted frames:

```
[nonce:12 bytes][ciphertext length:2 bytes BE][ciphertext + AEAD tag]
```

The nonce is transmitted with each frame. The ciphertext includes the AEAD authentication tag (16 bytes for both ChaCha20-Poly1305 and AES-256-GCM).

## Data frame plaintext format

Inside each encrypted frame, the plaintext is a data frame:

```
[command:1][flags:1][stream_id:4][payload:variable]
```

| Field | Size | Description |
|-------|------|-------------|
| `command` | 1 byte | Command type (see below) |
| `flags` | 1 byte | Bit flags (see below) |
| `stream_id` | 4 bytes (BE) | Stream multiplexing identifier |
| `payload` | variable | Command-specific data |

## Command types

| Code | Command | Direction | Payload |
|------|---------|-----------|---------|
| `0x01` | CONNECT | Client → Server | Destination address + port |
| `0x02` | DATA | Bidirectional | Raw data bytes |
| `0x03` | CLOSE | Bidirectional | None |
| `0x04` | PING | Bidirectional | Sequence number (4 bytes) |
| `0x05` | PONG | Bidirectional | Sequence number (4 bytes) |
| `0x06` | REGISTER_FORWARD | Client → Server | Remote port (2 bytes) + name |
| `0x07` | FORWARD_READY | Server → Client | Remote port (2 bytes) + success (1 byte) |
| `0x08` | FORWARD_CONNECT | Server → Client | Remote port (2 bytes) |

## Flag bits

| Bit | Name | Description |
|-----|------|-------------|
| `0x01` | PADDED | Frame contains padding |

## Protocol constants

| Constant | Value | Description |
|----------|-------|-------------|
| `PROTOCOL_VERSION` | `0x01` | Current protocol version |
| `MAX_FRAME_SIZE` | 16384 | Maximum frame size in bytes |
| `NONCE_SIZE` | 12 | Nonce size in bytes |
| `MAX_PADDING_SIZE` | 256 | Maximum padding size in bytes |
