//! Salamander UDP obfuscation (Hysteria2-style).
//!
//! Strips QUIC headers and makes packets look like random UDP traffic.
//! Uses BLAKE3 to derive a keystream from a shared password and packet length.
//! Obfuscation is stateless — both sides derive the same keystream
//! from the password and the packet size.
//!
//! Architecture:
//! ```text
//! Application <-> quinn (QUIC) <-> SalamanderSocket <-> UdpSocket <-> Network
//! ```

use std::fmt;
use std::io::{self, IoSliceMut};
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use quinn::udp::{RecvMeta, Transmit};
use quinn::{AsyncUdpSocket, UdpPoller};

/// Obfuscate a packet using XOR with a BLAKE3-derived keystream.
///
/// The keystream is derived from the password and packet length,
/// making the obfuscation stateless (no counter synchronization needed).
pub fn obfuscate(packet: &[u8], password: &[u8]) -> Vec<u8> {
    if packet.is_empty() {
        return Vec::new();
    }

    let keystream = derive_keystream(password, packet.len());
    xor_bytes(packet, &keystream)
}

/// Deobfuscate a packet (XOR is its own inverse).
pub fn deobfuscate(packet: &[u8], password: &[u8]) -> Vec<u8> {
    obfuscate(packet, password)
}

/// XOR a buffer in-place with the keystream derived from the password.
pub fn xor_in_place(data: &mut [u8], password: &[u8]) {
    if data.is_empty() {
        return;
    }
    let keystream = derive_keystream(password, data.len());
    for (d, k) in data.iter_mut().zip(keystream.iter()) {
        *d ^= k;
    }
}

/// Pre-derived BLAKE3 key for Salamander obfuscation.
/// Caches the derived key to avoid re-computing it per packet.
pub struct SalamanderKey {
    key: [u8; 32],
}

impl SalamanderKey {
    pub fn new(password: &[u8]) -> Self {
        Self {
            key: blake3::derive_key("prisma-salamander-v1", password),
        }
    }

    /// Derive a keystream of the given length.
    pub fn keystream(&self, len: usize) -> Vec<u8> {
        let hasher = blake3::Hasher::new_keyed(&self.key);
        let mut output = vec![0u8; len];
        hasher.finalize_xof().fill(&mut output);
        output
    }

    /// v4: Derive a keystream using a per-packet nonce for non-deterministic output.
    /// This prevents identical packets from producing identical obfuscated output.
    pub fn keystream_with_nonce(&self, len: usize, nonce: &[u8; 8]) -> Vec<u8> {
        let mut keyed_hasher = blake3::Hasher::new_keyed(&self.key);
        keyed_hasher.update(nonce);
        let mut output = vec![0u8; len];
        keyed_hasher.finalize_xof().fill(&mut output);
        output
    }

    /// XOR a buffer in-place with the derived keystream.
    pub fn xor_in_place(&self, data: &mut [u8]) {
        if data.is_empty() {
            return;
        }
        let keystream = self.keystream(data.len());
        for (d, k) in data.iter_mut().zip(keystream.iter()) {
            *d ^= k;
        }
    }

    /// v4: XOR with a per-packet nonce for non-deterministic output.
    pub fn xor_in_place_with_nonce(&self, data: &mut [u8], nonce: &[u8; 8]) {
        if data.is_empty() {
            return;
        }
        let keystream = self.keystream_with_nonce(data.len(), nonce);
        for (d, k) in data.iter_mut().zip(keystream.iter()) {
            *d ^= k;
        }
    }
}

/// ASCII prefix length for GFW entropy exemption.
/// Re-uses the value from entropy module; defined here to avoid a cross-module const dep.
const ASCII_PREFIX_LEN: usize = crate::entropy::ASCII_PREFIX_LEN;

/// v4: Prepend an ASCII prefix to a packet before obfuscation.
/// This ensures the first bytes on the wire are printable ASCII,
/// passing GFW exemption rule Ex2.
pub fn prepend_ascii_prefix(data: &[u8]) -> Vec<u8> {
    let prefix = crate::entropy::generate_ascii_prefix();
    let mut result = Vec::with_capacity(ASCII_PREFIX_LEN + data.len());
    result.extend_from_slice(&prefix);
    result.extend_from_slice(data);
    result
}

/// v4: Strip the ASCII prefix from a received packet.
pub fn strip_ascii_prefix(data: &[u8]) -> Option<&[u8]> {
    if data.len() < ASCII_PREFIX_LEN {
        return None;
    }
    Some(&data[ASCII_PREFIX_LEN..])
}

/// v4: Obfuscate with per-packet nonce (non-deterministic).
/// Wire format: [nonce:8][obfuscated_data:var]
pub fn obfuscate_v4(packet: &[u8], password: &[u8]) -> Vec<u8> {
    if packet.is_empty() {
        return Vec::new();
    }

    let mut nonce = [0u8; 8];
    rand::Rng::fill(&mut rand::thread_rng(), &mut nonce[..]);

    let key = SalamanderKey::new(password);
    let keystream = key.keystream_with_nonce(packet.len(), &nonce);

    let mut result = Vec::with_capacity(8 + packet.len());
    result.extend_from_slice(&nonce);
    for (d, k) in packet.iter().zip(keystream.iter()) {
        result.push(d ^ k);
    }
    result
}

/// v4: Deobfuscate a packet with per-packet nonce.
pub fn deobfuscate_v4(data: &[u8], password: &[u8]) -> Option<Vec<u8>> {
    if data.len() < 8 {
        return None;
    }

    let mut nonce = [0u8; 8];
    nonce.copy_from_slice(&data[..8]);
    let ciphertext = &data[8..];

    let key = SalamanderKey::new(password);
    let keystream = key.keystream_with_nonce(ciphertext.len(), &nonce);

    let plaintext: Vec<u8> = ciphertext
        .iter()
        .zip(keystream.iter())
        .map(|(d, k)| d ^ k)
        .collect();
    Some(plaintext)
}

/// Derive a keystream of the given length from the password.
fn derive_keystream(password: &[u8], len: usize) -> Vec<u8> {
    SalamanderKey::new(password).keystream(len)
}

/// XOR two byte slices together.
fn xor_bytes(data: &[u8], key: &[u8]) -> Vec<u8> {
    data.iter().zip(key.iter()).map(|(&d, &k)| d ^ k).collect()
}

// ---------------------------------------------------------------------------
// SalamanderSocket: quinn AsyncUdpSocket wrapper
// ---------------------------------------------------------------------------

/// A UDP socket wrapper that transparently obfuscates/deobfuscates all packets.
///
/// Sits between quinn and the real UDP socket. Quinn sees normal QUIC packets,
/// but the network sees XOR-obfuscated random-looking bytes.
pub struct SalamanderSocket {
    inner: Arc<dyn AsyncUdpSocket>,
    cached_key: SalamanderKey,
}

impl SalamanderSocket {
    /// Wrap an existing `AsyncUdpSocket` with Salamander obfuscation.
    pub fn wrap(inner: Arc<dyn AsyncUdpSocket>, password: &[u8]) -> Arc<Self> {
        Arc::new(Self {
            inner,
            cached_key: SalamanderKey::new(password),
        })
    }
}

impl fmt::Debug for SalamanderSocket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SalamanderSocket")
            .field("inner", &self.inner)
            .finish()
    }
}

/// Poller that delegates to the inner socket's poller.
struct SalamanderPoller {
    inner: Pin<Box<dyn UdpPoller>>,
}

impl fmt::Debug for SalamanderPoller {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SalamanderPoller")
            .field("inner", &self.inner)
            .finish()
    }
}

impl UdpPoller for SalamanderPoller {
    fn poll_writable(self: Pin<&mut Self>, cx: &mut Context) -> Poll<io::Result<()>> {
        // SAFETY: we only access the inner field, which is pinned
        let this = self.get_mut();
        this.inner.as_mut().poll_writable(cx)
    }
}

impl AsyncUdpSocket for SalamanderSocket {
    fn create_io_poller(self: Arc<Self>) -> Pin<Box<dyn UdpPoller>> {
        let inner_poller = self.inner.clone().create_io_poller();
        Box::pin(SalamanderPoller {
            inner: inner_poller,
        })
    }

    fn try_send(&self, transmit: &Transmit) -> io::Result<()> {
        // v4: Single-buffer nonce-based obfuscation with ASCII prefix
        let payload = transmit.contents;
        let total_len = ASCII_PREFIX_LEN + 8 + payload.len();
        let mut buf = Vec::with_capacity(total_len);

        // ASCII prefix for GFW entropy exemption (always, avoids per-packet popcount scan)
        buf.extend_from_slice(&crate::entropy::generate_ascii_prefix());

        // 8-byte random nonce
        let mut nonce = [0u8; 8];
        rand::Rng::fill(&mut rand::thread_rng(), &mut nonce[..]);
        buf.extend_from_slice(&nonce);

        // XOR obfuscate payload directly into the buffer
        let keystream = self.cached_key.keystream_with_nonce(payload.len(), &nonce);
        for (d, k) in payload.iter().zip(keystream.iter()) {
            buf.push(d ^ k);
        }

        let obfuscated_transmit = Transmit {
            destination: transmit.destination,
            ecn: transmit.ecn,
            contents: &buf,
            segment_size: transmit.segment_size,
            src_ip: transmit.src_ip,
        };
        self.inner.try_send(&obfuscated_transmit)
    }

    fn poll_recv(
        &self,
        cx: &mut Context,
        bufs: &mut [IoSliceMut<'_>],
        meta: &mut [RecvMeta],
    ) -> Poll<io::Result<usize>> {
        // Receive from the inner socket
        let result = self.inner.poll_recv(cx, bufs, meta);

        // If we got data, strip ASCII prefix (if present) and deobfuscate with nonce
        if let Poll::Ready(Ok(count)) = &result {
            for i in 0..*count {
                let data_len = meta[i].len;
                let buf = &mut bufs[i];

                // Always expect ASCII prefix + nonce (v4 always sends prefix).
                // Fall back to nonce-at-offset-0 for backward compat with older senders.
                let (nonce_start, has_prefix) = if data_len >= ASCII_PREFIX_LEN + 8
                    && crate::entropy::has_ascii_prefix(&buf[..data_len], ASCII_PREFIX_LEN)
                {
                    (ASCII_PREFIX_LEN, true)
                } else if data_len >= 8 {
                    (0, false)
                } else {
                    continue;
                };

                // Extract 8-byte nonce
                let mut nonce = [0u8; 8];
                nonce.copy_from_slice(&buf[nonce_start..nonce_start + 8]);
                let payload_start = nonce_start + 8;
                let payload_len = if has_prefix {
                    data_len - ASCII_PREFIX_LEN - 8
                } else {
                    data_len - 8
                };

                // Deobfuscate payload in-place
                self.cached_key.xor_in_place_with_nonce(
                    &mut buf[payload_start..payload_start + payload_len],
                    &nonce,
                );

                // Shift deobfuscated payload to the beginning of the buffer
                if payload_start > 0 {
                    buf.copy_within(payload_start..payload_start + payload_len, 0);
                }
                meta[i].len = payload_len;
                meta[i].stride = meta[i].stride.min(payload_len);
            }
        }

        result
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        self.inner.local_addr()
    }

    fn max_transmit_segments(&self) -> usize {
        self.inner.max_transmit_segments()
    }

    fn max_receive_segments(&self) -> usize {
        self.inner.max_receive_segments()
    }

    fn may_fragment(&self) -> bool {
        self.inner.may_fragment()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_round_trip() {
        let password = b"test-password-123";
        let original = b"Hello, World! This is a QUIC packet.";

        let obfuscated = obfuscate(original, password);
        let recovered = deobfuscate(&obfuscated, password);

        assert_eq!(&recovered, original);
    }

    #[test]
    fn test_obfuscated_differs_from_original() {
        let password = b"test-password";
        let original = b"QUIC header data that should be obfuscated";

        let obfuscated = obfuscate(original, password);
        assert_ne!(&obfuscated[..], &original[..]);
    }

    #[test]
    fn test_different_passwords_different_output() {
        let original = b"Same packet data";

        let o1 = obfuscate(original, b"password-1");
        let o2 = obfuscate(original, b"password-2");

        assert_ne!(o1, o2);
    }

    #[test]
    fn test_empty_packet() {
        let result = obfuscate(b"", b"password");
        assert!(result.is_empty());
    }

    #[test]
    fn test_deterministic() {
        let password = b"deterministic-test";
        let original = b"same input produces same output";

        let o1 = obfuscate(original, password);
        let o2 = obfuscate(original, password);

        assert_eq!(o1, o2);
    }

    #[test]
    fn test_large_packet() {
        let password = b"large-packet-test";
        let original = vec![0xAB; 1500]; // MTU-sized packet

        let obfuscated = obfuscate(&original, password);
        let recovered = deobfuscate(&obfuscated, password);

        assert_eq!(recovered, original);
        assert_eq!(obfuscated.len(), original.len());
    }

    #[test]
    fn test_xor_in_place() {
        let password = b"in-place-test";
        let original = b"data to obfuscate in place".to_vec();
        let mut data = original.clone();

        xor_in_place(&mut data, password);
        assert_ne!(&data, &original);

        // XOR again to recover
        xor_in_place(&mut data, password);
        assert_eq!(&data, &original);
    }

    #[test]
    fn test_v4_round_trip() {
        let password = b"v4-test-password";
        let original = b"Hello from v4 with per-packet nonce!";

        let obfuscated = obfuscate_v4(original, password);
        // Should be 8 bytes longer (nonce prefix)
        assert_eq!(obfuscated.len(), original.len() + 8);

        let recovered = deobfuscate_v4(&obfuscated, password).unwrap();
        assert_eq!(&recovered, &original[..]);
    }

    #[test]
    fn test_v4_non_deterministic() {
        let password = b"v4-nondeterminism";
        let original = b"same input different output";

        let o1 = obfuscate_v4(original, password);
        let o2 = obfuscate_v4(original, password);

        // Same password + same input should produce DIFFERENT output (random nonce)
        assert_ne!(o1, o2, "v4 obfuscation should be non-deterministic");

        // But both should deobfuscate to the same plaintext
        let r1 = deobfuscate_v4(&o1, password).unwrap();
        let r2 = deobfuscate_v4(&o2, password).unwrap();
        assert_eq!(r1, r2);
        assert_eq!(&r1, &original[..]);
    }

    #[test]
    fn test_ascii_prefix() {
        let data = b"packet data";
        let prefixed = prepend_ascii_prefix(data);

        // Should be 8 bytes longer
        assert_eq!(prefixed.len(), data.len() + 8);

        // First 8 bytes should be printable ASCII
        for &b in &prefixed[..8] {
            assert!(
                b >= 0x20 && b <= 0x7E,
                "byte {:#04x} not printable ASCII",
                b
            );
        }

        // Stripping should recover original
        let stripped = strip_ascii_prefix(&prefixed).unwrap();
        assert_eq!(stripped, data);
    }

    #[test]
    fn test_per_packet_nonce_keystream() {
        let key = SalamanderKey::new(b"nonce-test");
        let nonce1 = [0u8; 8];
        let nonce2 = [1u8; 8];

        let ks1 = key.keystream_with_nonce(32, &nonce1);
        let ks2 = key.keystream_with_nonce(32, &nonce2);

        assert_ne!(
            ks1, ks2,
            "Different nonces must produce different keystreams"
        );
    }
}
