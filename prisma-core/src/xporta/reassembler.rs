use std::collections::BTreeMap;
use std::time::Instant;

use super::types::{REASSEMBLER_GAP_TIMEOUT_SECS, REASSEMBLER_MAX_BUFFER};

/// Reorders out-of-sequence data chunks and delivers them in order.
///
/// Since XPorta uses multiple concurrent HTTP streams, data may arrive out of order.
/// The Reassembler buffers out-of-order chunks and delivers them sequentially.
pub struct Reassembler {
    /// Next expected sequence number.
    next_seq: u32,
    /// Buffered out-of-order chunks, keyed by sequence number.
    buffer: BTreeMap<u32, Vec<u8>>,
    /// Timestamp of last successful in-order delivery (for gap timeout detection).
    last_delivery: Instant,
}

impl Reassembler {
    /// Create a new reassembler starting at sequence 0.
    pub fn new() -> Self {
        Self {
            next_seq: 0,
            buffer: BTreeMap::new(),
            last_delivery: Instant::now(),
        }
    }

    /// Insert a chunk with its sequence number.
    ///
    /// Returns an error if the buffer is full (max 64 entries) — indicates a serious problem.
    pub fn insert(&mut self, seq: u32, data: Vec<u8>) -> Result<(), ReassemblerError> {
        if seq < self.next_seq {
            // Duplicate or already-delivered — ignore silently
            return Ok(());
        }

        if seq == self.next_seq {
            // This is the next expected chunk — no need to buffer it
            // (it will be collected by drain())
            self.buffer.insert(seq, data);
        } else {
            // Out of order — buffer it
            if self.buffer.len() >= REASSEMBLER_MAX_BUFFER {
                return Err(ReassemblerError::BufferFull);
            }
            self.buffer.insert(seq, data);
        }

        Ok(())
    }

    /// Drain all contiguous chunks starting from `next_seq`.
    ///
    /// Returns the in-order data that can be delivered.
    pub fn drain(&mut self) -> Vec<Vec<u8>> {
        let mut result = Vec::new();

        while let Some(data) = self.buffer.remove(&self.next_seq) {
            result.push(data);
            self.next_seq = self.next_seq.wrapping_add(1);
            self.last_delivery = Instant::now();
        }

        result
    }

    /// Check if there's a gap that has persisted too long.
    pub fn has_gap_timeout(&self) -> bool {
        if self.buffer.is_empty() {
            return false;
        }
        // If we have buffered data but haven't been able to deliver, check timeout
        self.last_delivery.elapsed().as_secs() > REASSEMBLER_GAP_TIMEOUT_SECS
    }

    /// Get the next expected sequence number.
    pub fn next_seq(&self) -> u32 {
        self.next_seq
    }

    /// Number of buffered out-of-order chunks.
    pub fn buffered_count(&self) -> usize {
        self.buffer.len()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ReassemblerError {
    #[error("reassembler buffer full ({} entries)", REASSEMBLER_MAX_BUFFER)]
    BufferFull,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_in_order_delivery() {
        let mut r = Reassembler::new();
        r.insert(0, b"zero".to_vec()).unwrap();
        r.insert(1, b"one".to_vec()).unwrap();
        r.insert(2, b"two".to_vec()).unwrap();

        let chunks = r.drain();
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0], b"zero");
        assert_eq!(chunks[1], b"one");
        assert_eq!(chunks[2], b"two");
        assert_eq!(r.next_seq(), 3);
    }

    #[test]
    fn test_out_of_order_delivery() {
        let mut r = Reassembler::new();

        // Arrive out of order: 2, 0, 1
        r.insert(2, b"two".to_vec()).unwrap();
        let chunks = r.drain();
        assert!(chunks.is_empty()); // Can't deliver yet

        r.insert(0, b"zero".to_vec()).unwrap();
        let chunks = r.drain();
        assert_eq!(chunks.len(), 1); // Only seq 0
        assert_eq!(chunks[0], b"zero");

        r.insert(1, b"one".to_vec()).unwrap();
        let chunks = r.drain();
        assert_eq!(chunks.len(), 2); // Now 1 and 2 are contiguous
        assert_eq!(chunks[0], b"one");
        assert_eq!(chunks[1], b"two");
        assert_eq!(r.next_seq(), 3);
    }

    #[test]
    fn test_duplicate_ignored() {
        let mut r = Reassembler::new();
        r.insert(0, b"zero".to_vec()).unwrap();
        r.drain();

        // Insert duplicate (already delivered)
        r.insert(0, b"zero-dup".to_vec()).unwrap();
        let chunks = r.drain();
        assert!(chunks.is_empty());
        assert_eq!(r.next_seq(), 1);
    }

    #[test]
    fn test_buffer_full() {
        let mut r = Reassembler::new();

        // Fill buffer with out-of-order entries (skip seq 0)
        for i in 1..=REASSEMBLER_MAX_BUFFER as u32 {
            r.insert(i, vec![i as u8]).unwrap();
        }

        // One more should fail
        let result = r.insert(REASSEMBLER_MAX_BUFFER as u32 + 1, vec![0xFF]);
        assert!(result.is_err());
    }

    #[test]
    fn test_gap_timeout() {
        let mut r = Reassembler::new();

        // Insert an out-of-order chunk
        r.insert(5, b"five".to_vec()).unwrap();

        // Just created — no timeout yet
        assert!(!r.has_gap_timeout());

        // Artificially set last_delivery to long ago
        r.last_delivery = Instant::now() - std::time::Duration::from_secs(REASSEMBLER_GAP_TIMEOUT_SECS + 1);
        assert!(r.has_gap_timeout());
    }

    #[test]
    fn test_empty_buffer_no_timeout() {
        let mut r = Reassembler::new();
        r.last_delivery = Instant::now() - std::time::Duration::from_secs(100);
        // Empty buffer — no gap, no timeout
        assert!(!r.has_gap_timeout());
    }

    #[test]
    fn test_interleaved_insert_drain() {
        let mut r = Reassembler::new();

        r.insert(0, b"a".to_vec()).unwrap();
        assert_eq!(r.drain().len(), 1);

        r.insert(3, b"d".to_vec()).unwrap();
        r.insert(1, b"b".to_vec()).unwrap();
        assert_eq!(r.drain().len(), 1); // Only seq 1

        r.insert(2, b"c".to_vec()).unwrap();
        let chunks = r.drain();
        assert_eq!(chunks.len(), 2); // seq 2 and 3
        assert_eq!(chunks[0], b"c");
        assert_eq!(chunks[1], b"d");
        assert_eq!(r.next_seq(), 4);
    }
}
