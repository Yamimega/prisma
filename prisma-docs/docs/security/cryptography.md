---
sidebar_position: 2
---

# Cryptography

Prisma uses modern, well-audited cryptographic primitives for all security-critical operations. This page documents the cryptographic components and their roles.

## Overview

| Component | Algorithm | Purpose |
|-----------|-----------|---------|
| Key exchange | X25519 ECDH | Ephemeral shared secret per session |
| Key derivation | BLAKE3 `derive_key` | Session key from shared secret + context |
| Data encryption | ChaCha20-Poly1305 or AES-256-GCM | Authenticated encryption of data frames |
| Authentication | HMAC-SHA256 | Client identity verification |
| Challenge-response | BLAKE3 hash | Proves client derived the correct session key |
| Nonce construction | Counter-based | Per-direction monotonic nonce |
| Anti-replay | 1024-bit sliding bitmap | Detects replayed or out-of-order frames |

## X25519 ECDH key exchange

Each session begins with an ephemeral X25519 Diffie-Hellman key exchange. Both the client and server generate fresh key pairs using OS randomness (`OsRng`), ensuring forward secrecy — compromising long-term secrets does not reveal past session keys.

The shared secret is computed as:

```
shared_secret = X25519(my_secret, their_public)
```

Both sides arrive at the same 32-byte shared secret.

## BLAKE3 key derivation

The session key is derived from the shared secret using BLAKE3's key derivation mode with domain separation:

- **Domain string:** `"prisma-veil-v1-session-key"`
- **Context (104 bytes):** `shared_secret (32) || client_pub (32) || server_pub (32) || timestamp (8)`

```
session_key = BLAKE3_derive_key(
    domain = "prisma-veil-v1-session-key",
    context = shared_secret || client_pub || server_pub || timestamp
)
```

Including both public keys and the timestamp in the context ensures unique session keys even in the unlikely event of ephemeral key reuse.

## AEAD encryption

All data frames are encrypted with an AEAD cipher chosen by the client during the handshake:

### ChaCha20-Poly1305 (default)

- 256-bit key, 96-bit nonce, 128-bit authentication tag
- Software-optimized — fast on platforms without AES hardware acceleration
- Default cipher suite

### AES-256-GCM

- 256-bit key, 96-bit nonce, 128-bit authentication tag
- Hardware-accelerated on platforms with AES-NI instructions
- Alternative cipher suite

Both ciphers provide authenticated encryption with associated data (AEAD), ensuring confidentiality and integrity of every frame.

## HMAC-SHA256 authentication

Client identity is verified using HMAC-SHA256. During the handshake, the client computes:

```
auth_token = HMAC-SHA256(auth_secret, client_id || timestamp)
```

The server independently computes the same token using its stored copy of the auth secret and verifies equality using constant-time comparison.

## Challenge-response

During the handshake, the server sends an encrypted random challenge. The client proves it derived the correct session key by:

1. Decrypting the challenge using the derived session key
2. Computing `challenge_response = BLAKE3(challenge)`
3. Sending the response back in the encrypted `ClientAuth` message

The server verifies the response matches its expected value.

## Nonce structure

Nonces are 12 bytes (96 bits), structured as:

```
[direction:1][reserved:3][counter:8]
```

| Field | Size | Description |
|-------|------|-------------|
| `direction` | 1 byte | `0x00` = client→server, `0x01` = server→client |
| `reserved` | 3 bytes | Zero-filled |
| `counter` | 8 bytes (BE) | Monotonically increasing counter |

The direction byte ensures that even with the same counter value, client-to-server and server-to-client nonces are always distinct. This prevents nonce reuse across directions.

## Constant-time comparison

All security-sensitive comparisons (auth tokens, challenge responses) use constant-time equality checks via the `subtle` crate. This prevents timing side-channel attacks that could leak information about secret values.
