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
    if packet.is_empty() {
        return Vec::new();
    }

    let keystream = derive_keystream(password, packet.len());
    xor_bytes(packet, &keystream)
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
        // Copy and XOR in-place to avoid separate keystream allocation + collect
        let mut obfuscated = transmit.contents.to_vec();
        self.cached_key.xor_in_place(&mut obfuscated);
        let obfuscated_transmit = Transmit {
            destination: transmit.destination,
            ecn: transmit.ecn,
            contents: &obfuscated,
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

        // If we got data, deobfuscate it in-place
        if let Poll::Ready(Ok(count)) = &result {
            for i in 0..*count {
                let len = meta[i].len;
                let stride = meta[i].stride;
                // Handle GRO: multiple datagrams may be packed in one buffer
                if stride > 0 && stride < len {
                    // Multiple datagrams, deobfuscate each one
                    let buf = &mut bufs[i];
                    let mut offset = 0;
                    while offset < len {
                        let end = (offset + stride).min(len);
                        self.cached_key.xor_in_place(&mut buf[offset..end]);
                        offset += stride;
                    }
                } else {
                    // Single datagram
                    self.cached_key.xor_in_place(&mut bufs[i][..len]);
                }
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
}
