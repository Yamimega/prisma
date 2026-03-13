//! Forward Error Correction (FEC) using Reed-Solomon erasure coding.
//!
//! Adds redundancy to UDP datagram groups so that lost packets can be
//! recovered without retransmission. Each FEC group contains `data_shards`
//! original packets plus `parity_shards` parity packets. If any
//! `data_shards` packets out of the total are received, all original
//! data can be reconstructed.
//!
//! Wire format per FEC-enabled datagram:
//! ```text
//! [assoc_id:4][seq:4][fec_group:2][fec_index:1][fec_total:1][payload:var]
//! ```

use reed_solomon_erasure::galois_8::ReedSolomon;
use serde::{Deserialize, Serialize};

/// FEC configuration (per-flow).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FecConfig {
    #[serde(default)]
    pub enabled: bool,
    /// Number of original data packets per group.
    #[serde(default = "default_data_shards")]
    pub data_shards: usize,
    /// Number of parity packets per group (overhead).
    #[serde(default = "default_parity_shards")]
    pub parity_shards: usize,
}

impl Default for FecConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            data_shards: default_data_shards(),
            parity_shards: default_parity_shards(),
        }
    }
}

fn default_data_shards() -> usize {
    10
}
fn default_parity_shards() -> usize {
    3
}

/// FEC encoder: collects data shards and produces parity shards.
pub struct FecEncoder {
    rs: ReedSolomon,
    data_shards: usize,
    parity_shards: usize,
    group_id: u16,
    shard_size: usize,
    buffer: Vec<Option<Vec<u8>>>,
    count: usize,
}

impl FecEncoder {
    pub fn new(data_shards: usize, parity_shards: usize) -> Self {
        let rs = ReedSolomon::new(data_shards, parity_shards).expect("invalid FEC shard counts");
        let total = data_shards + parity_shards;
        Self {
            rs,
            data_shards,
            parity_shards,
            group_id: 0,
            shard_size: 0,
            buffer: vec![None; total],
            count: 0,
        }
    }

    /// Add a data shard. Returns `None` if the group is not yet full,
    /// or `Some(parity_shards)` when the group is complete.
    ///
    /// All shards in a group must be padded to the same size (the maximum
    /// payload size in the group). The caller should zero-pad shorter payloads.
    pub fn add_shard(&mut self, data: &[u8]) -> Option<FecGroup> {
        if self.count == 0 {
            self.shard_size = data.len();
        } else {
            self.shard_size = self.shard_size.max(data.len());
        }

        let idx = self.count;
        self.buffer[idx] = Some(data.to_vec());
        self.count += 1;

        if self.count == self.data_shards {
            // Pad all shards to the same size
            for s in self.buffer.iter_mut().take(self.data_shards).flatten() {
                s.resize(self.shard_size, 0);
            }
            // Initialize parity shards
            for i in self.data_shards..self.data_shards + self.parity_shards {
                self.buffer[i] = Some(vec![0u8; self.shard_size]);
            }

            // Encode parity
            let mut shards: Vec<&mut [u8]> = self
                .buffer
                .iter_mut()
                .map(|s| s.as_mut().unwrap().as_mut_slice())
                .collect();
            self.rs.encode(&mut shards).expect("FEC encode failed");

            let group = FecGroup {
                group_id: self.group_id,
                data_shards: self.data_shards as u8,
                parity_shards: self.parity_shards as u8,
                shard_size: self.shard_size,
                shards: self.buffer.iter().map(|s| s.clone().unwrap()).collect(),
            };

            // Reset for next group
            self.group_id = self.group_id.wrapping_add(1);
            self.count = 0;
            for s in self.buffer.iter_mut() {
                *s = None;
            }

            Some(group)
        } else {
            None
        }
    }

    /// Flush any partial group (returns data shards without parity).
    /// Used when the stream ends with fewer than `data_shards` packets.
    pub fn flush(&mut self) -> Option<FecGroup> {
        if self.count == 0 {
            return None;
        }

        let group = FecGroup {
            group_id: self.group_id,
            data_shards: self.count as u8,
            parity_shards: 0,
            shard_size: self.shard_size,
            shards: self.buffer[..self.count]
                .iter()
                .filter_map(|s| s.clone())
                .collect(),
        };

        self.group_id = self.group_id.wrapping_add(1);
        self.count = 0;
        for s in self.buffer.iter_mut() {
            *s = None;
        }

        Some(group)
    }
}

/// A complete FEC group with data + parity shards.
#[derive(Debug, Clone)]
pub struct FecGroup {
    pub group_id: u16,
    pub data_shards: u8,
    pub parity_shards: u8,
    pub shard_size: usize,
    pub shards: Vec<Vec<u8>>,
}

/// FEC decoder: collects received shards and reconstructs missing data.
pub struct FecDecoder {
    rs: ReedSolomon,
    data_shards: usize,
    parity_shards: usize,
    /// Per-group receive buffer: group_id -> (shards, received_mask)
    groups: std::collections::HashMap<u16, GroupBuffer>,
    /// Maximum number of incomplete groups to keep before auto-eviction.
    max_groups: usize,
    /// Highest group_id seen (for staleness detection).
    highest_group: u16,
}

struct GroupBuffer {
    shards: Vec<Option<Vec<u8>>>,
    shard_size: usize,
    received: usize,
}

impl FecDecoder {
    pub fn new(data_shards: usize, parity_shards: usize) -> Self {
        let rs = ReedSolomon::new(data_shards, parity_shards).expect("invalid FEC shard counts");
        Self {
            rs,
            data_shards,
            parity_shards,
            groups: std::collections::HashMap::new(),
            max_groups: 512,
            highest_group: 0,
        }
    }

    /// Feed a received shard. Returns `Some(data_shards)` if the group
    /// can be fully reconstructed, `None` otherwise.
    pub fn add_shard(
        &mut self,
        group_id: u16,
        shard_index: u8,
        shard_data: &[u8],
    ) -> Option<Vec<Vec<u8>>> {
        let total = self.data_shards + self.parity_shards;
        let idx = shard_index as usize;
        if idx >= total {
            return None;
        }

        // Track highest group and auto-evict stale groups
        if group_id.wrapping_sub(self.highest_group) < 32768 {
            self.highest_group = group_id;
        }
        if self.groups.len() >= self.max_groups {
            self.evict_before(self.highest_group.wrapping_sub(256));
        }

        let buf = self.groups.entry(group_id).or_insert_with(|| GroupBuffer {
            shards: vec![None; total],
            shard_size: 0,
            received: 0,
        });

        if buf.shards[idx].is_some() {
            return None; // duplicate
        }

        buf.shard_size = buf.shard_size.max(shard_data.len());
        buf.shards[idx] = Some(shard_data.to_vec());
        buf.received += 1;

        // Need at least data_shards to reconstruct
        if buf.received >= self.data_shards {
            // Pad all received shards to same size
            let shard_size = buf.shard_size;
            for s in buf.shards.iter_mut().flatten() {
                s.resize(shard_size, 0);
            }

            // Attempt reconstruction
            if self.rs.reconstruct(&mut buf.shards).is_ok() {
                let data: Vec<Vec<u8>> = buf.shards[..self.data_shards]
                    .iter()
                    .map(|s| s.clone().unwrap())
                    .collect();
                self.groups.remove(&group_id);
                return Some(data);
            }
        }

        None
    }

    /// Remove stale groups that will never complete.
    pub fn evict_before(&mut self, min_group_id: u16) {
        self.groups.retain(|&id, _| {
            // Handle wrapping: keep if id >= min or if the gap is small (wraparound)
            let diff = id.wrapping_sub(min_group_id);
            diff < 256 // keep groups within 256 of min
        });
    }
}

/// Encode an FEC shard header (prepended to each datagram in an FEC group).
///
/// Format: `[fec_group:2 LE][fec_index:1][fec_total:1]` = 4 bytes
pub fn encode_fec_header(group_id: u16, index: u8, total: u8) -> [u8; 4] {
    let mut buf = [0u8; 4];
    buf[0..2].copy_from_slice(&group_id.to_le_bytes());
    buf[2] = index;
    buf[3] = total;
    buf
}

/// Decode an FEC shard header from a 4-byte prefix.
pub fn decode_fec_header(buf: &[u8; 4]) -> (u16, u8, u8) {
    let group_id = u16::from_le_bytes([buf[0], buf[1]]);
    let index = buf[2];
    let total = buf[3];
    (group_id, index, total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_decode_full_group() {
        let mut encoder = FecEncoder::new(3, 2);
        let data = vec![
            b"hello world!!".to_vec(),
            b"second packet".to_vec(),
            b"third  packet".to_vec(),
        ];

        let mut group = None;
        for d in &data {
            group = encoder.add_shard(d);
        }
        let group = group.expect("should produce group after 3 shards");
        assert_eq!(group.shards.len(), 5); // 3 data + 2 parity
        assert_eq!(group.data_shards, 3);
        assert_eq!(group.parity_shards, 2);
    }

    #[test]
    fn test_reconstruct_missing_shard() {
        let mut encoder = FecEncoder::new(3, 2);
        let data = vec![
            b"aaaaaaaaaaaa".to_vec(),
            b"bbbbbbbbbbbb".to_vec(),
            b"cccccccccccc".to_vec(),
        ];

        let mut group = None;
        for d in &data {
            group = encoder.add_shard(d);
        }
        let group = group.unwrap();

        // Simulate losing shard 1 (second data shard)
        let mut decoder = FecDecoder::new(3, 2);
        assert!(decoder
            .add_shard(group.group_id, 0, &group.shards[0])
            .is_none());
        // Skip shard 1
        assert!(decoder
            .add_shard(group.group_id, 2, &group.shards[2])
            .is_none());
        // Add parity shard 0 (index 3) — now we have 3 shards, enough to reconstruct
        let result = decoder.add_shard(group.group_id, 3, &group.shards[3]);
        let recovered = result.expect("should reconstruct with 3 of 5 shards");

        assert_eq!(recovered[0], data[0]);
        assert_eq!(recovered[1], data[1]);
        assert_eq!(recovered[2], data[2]);
    }

    #[test]
    fn test_reconstruct_missing_two_shards() {
        let mut encoder = FecEncoder::new(3, 2);
        let data = vec![
            b"packet-one..".to_vec(),
            b"packet-two..".to_vec(),
            b"packet-three".to_vec(),
        ];

        let mut group = None;
        for d in &data {
            group = encoder.add_shard(d);
        }
        let group = group.unwrap();

        // Lose shards 0 and 2 — only have shard 1, parity 3, parity 4
        let mut decoder = FecDecoder::new(3, 2);
        assert!(decoder
            .add_shard(group.group_id, 1, &group.shards[1])
            .is_none());
        assert!(decoder
            .add_shard(group.group_id, 3, &group.shards[3])
            .is_none());
        let result = decoder.add_shard(group.group_id, 4, &group.shards[4]);
        let recovered = result.expect("should reconstruct with 3 of 5 shards");

        assert_eq!(recovered[0], data[0]);
        assert_eq!(recovered[1], data[1]);
        assert_eq!(recovered[2], data[2]);
    }

    #[test]
    fn test_too_few_shards() {
        let mut encoder = FecEncoder::new(3, 2);
        let data = vec![b"aaaa".to_vec(), b"bbbb".to_vec(), b"cccc".to_vec()];

        let mut group = None;
        for d in &data {
            group = encoder.add_shard(d);
        }
        let group = group.unwrap();

        // Only 2 shards received (need 3)
        let mut decoder = FecDecoder::new(3, 2);
        assert!(decoder
            .add_shard(group.group_id, 0, &group.shards[0])
            .is_none());
        assert!(decoder
            .add_shard(group.group_id, 3, &group.shards[3])
            .is_none());
        // Still only 2 — can't reconstruct yet
    }

    #[test]
    fn test_fec_header_round_trip() {
        let header = encode_fec_header(0x1234, 5, 13);
        let (group, index, total) = decode_fec_header(&header);
        assert_eq!(group, 0x1234);
        assert_eq!(index, 5);
        assert_eq!(total, 13);
    }

    #[test]
    fn test_flush_partial_group() {
        let mut encoder = FecEncoder::new(4, 2);
        encoder.add_shard(b"first");
        encoder.add_shard(b"second");

        let group = encoder.flush().expect("should flush partial group");
        assert_eq!(group.data_shards, 2);
        assert_eq!(group.parity_shards, 0);
        assert_eq!(group.shards.len(), 2);
    }

    #[test]
    fn test_variable_size_shards() {
        let mut encoder = FecEncoder::new(3, 1);
        let data = vec![b"short".to_vec(), b"medium-length".to_vec(), b"a".to_vec()];

        let mut group = None;
        for d in &data {
            group = encoder.add_shard(d);
        }
        let group = group.unwrap();

        // All shards should be padded to max size
        assert!(group.shards.iter().all(|s| s.len() == group.shard_size));
        assert_eq!(group.shard_size, 13); // "medium-length".len()

        // Reconstruct after losing shard 0
        let mut decoder = FecDecoder::new(3, 1);
        assert!(decoder
            .add_shard(group.group_id, 1, &group.shards[1])
            .is_none());
        assert!(decoder
            .add_shard(group.group_id, 2, &group.shards[2])
            .is_none());
        let result = decoder.add_shard(group.group_id, 3, &group.shards[3]);
        let recovered = result.expect("should reconstruct");

        // Recovered data is padded — original content is prefix
        assert!(recovered[0].starts_with(b"short"));
        assert!(recovered[1].starts_with(b"medium-length"));
        assert!(recovered[2].starts_with(b"a"));
    }

    #[test]
    fn test_multiple_groups() {
        let mut encoder = FecEncoder::new(2, 1);

        // Group 0
        encoder.add_shard(b"g0-s0");
        let g0 = encoder.add_shard(b"g0-s1").expect("group 0 complete");
        assert_eq!(g0.group_id, 0);

        // Group 1
        encoder.add_shard(b"g1-s0");
        let g1 = encoder.add_shard(b"g1-s1").expect("group 1 complete");
        assert_eq!(g1.group_id, 1);
    }
}
