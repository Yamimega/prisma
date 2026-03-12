---
sidebar_position: 3
---

# Anti-Replay & Padding

Prisma includes two defenses against network-level attacks: a sliding-window anti-replay mechanism and random padding on handshake messages.

## Anti-replay protection

### Sliding window

Prisma uses a 1024-bit sliding window to detect replayed or reordered nonce counter values. The window is implemented as a bitmap of 16 x `u64` words (1024 bits total).

### How it works

- The window tracks a `base` counter and a 1024-bit bitmap of seen counters relative to that base
- Each incoming frame's nonce counter is checked against the window:
  - **Below `base`**: rejected as too old (more than 1024 frames behind the highest seen)
  - **Within the window and already marked**: rejected as a replay
  - **Within the window and not seen**: accepted and marked in the bitmap
  - **Above the window**: the window advances, old entries are discarded, and the new counter is accepted

### Properties

- Allows up to 1024 frames of out-of-order delivery
- Detects exact replays within the window
- Automatically evicts stale entries as the counter advances
- Zero heap allocation — fixed-size `[u64; 16]` array

### Error handling

When a replay is detected, the frame is rejected with a `ReplayDetected` error containing the offending counter value. The connection can continue processing subsequent valid frames.

## Random padding

### Handshake padding

Both `ClientHello` and `ServerHello` messages include random padding to resist traffic fingerprinting. The padding format is:

```
[padding_len:2 bytes BE][random_bytes:padding_len]
```

- Padding length is randomly chosen from 0 to `max_size` bytes (up to 256)
- Padding bytes are filled with cryptographically random data
- The receiver strips padding by reading the 2-byte length prefix

### Why padding matters

Without padding, handshake messages have predictable sizes based on the fixed-size fields (public keys, timestamps). An observer could fingerprint PrismaVeil traffic by matching these sizes. Random padding makes each handshake a different total size.

## Protocol constants

| Constant | Value | Description |
|----------|-------|-------------|
| `MAX_PADDING_SIZE` | 256 | Maximum padding size in bytes |
| Window size | 1024 bits | Anti-replay window capacity |
| Bitmap words | 16 | Number of `u64` words in the bitmap |
