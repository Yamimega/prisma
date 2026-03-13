use rand::Rng;
use serde_json;

use super::types::*;

/// Generate random padding string of given length.
fn random_padding(max_len: usize) -> String {
    if max_len == 0 {
        return String::new();
    }
    let mut rng = rand::thread_rng();
    let len = rng.gen_range(8..=max_len);
    (0..len)
        .map(|_| rng.gen_range(b'a'..=b'z') as char)
        .collect()
}

// --- JSON Encoding ---

/// Encode an upload request (JSON mode).
pub fn encode_request(seq: u32, payload: &[u8], encoding: XPortaEncoding) -> Vec<u8> {
    match encoding {
        XPortaEncoding::Json => {
            let req = UploadRequest {
                s: seq,
                d: base64_encode(payload),
                p: random_padding(32),
            };
            serde_json::to_vec(&req).expect("JSON serialization cannot fail")
        }
        XPortaEncoding::Binary => encode_binary_upload(seq, payload),
    }
}

/// Decode an upload request.
pub fn decode_request(data: &[u8], encoding: XPortaEncoding) -> Option<(u32, Vec<u8>)> {
    match encoding {
        XPortaEncoding::Json => {
            let req: UploadRequest = serde_json::from_slice(data).ok()?;
            let payload = base64_decode(&req.d)?;
            Some((req.s, payload))
        }
        XPortaEncoding::Binary => decode_binary_frame(data),
    }
}

/// Encode an upload response (JSON mode), optionally with piggyback download data.
pub fn encode_response(
    dl_seq: Option<u32>,
    dl_data: Option<&[u8]>,
    encoding: XPortaEncoding,
) -> Vec<u8> {
    match encoding {
        XPortaEncoding::Json => {
            let resp = UploadResponse {
                ok: true,
                s: dl_seq,
                d: dl_data.map(base64_encode),
                p: random_padding(32),
            };
            serde_json::to_vec(&resp).expect("JSON serialization cannot fail")
        }
        XPortaEncoding::Binary => {
            if let (Some(seq), Some(data)) = (dl_seq, dl_data) {
                encode_binary_download(seq, data)
            } else {
                // Empty OK response in binary: just 0 length
                vec![0u8; 8]
            }
        }
    }
}

/// Decode an upload response (extract piggyback download data).
pub fn decode_response(
    data: &[u8],
    encoding: XPortaEncoding,
) -> Option<(Option<u32>, Option<Vec<u8>>)> {
    match encoding {
        XPortaEncoding::Json => {
            let resp: UploadResponse = serde_json::from_slice(data).ok()?;
            let dl_data = resp.d.as_deref().and_then(base64_decode);
            Some((resp.s, dl_data))
        }
        XPortaEncoding::Binary => {
            if data.len() < 8 {
                return Some((None, None));
            }
            if let Some((seq, payload)) = decode_binary_frame(data) {
                if payload.is_empty() {
                    Some((None, None))
                } else {
                    Some((Some(seq), Some(payload)))
                }
            } else {
                Some((None, None))
            }
        }
    }
}

/// Encode a poll response with multiple items.
pub fn encode_poll_response(items: &[(u32, &[u8])]) -> Vec<u8> {
    let poll_items: Vec<PollItem> = items
        .iter()
        .map(|(seq, data)| PollItem {
            s: *seq,
            d: base64_encode(data),
        })
        .collect();
    let resp = PollResponse {
        items: poll_items,
        p: random_padding(32),
    };
    serde_json::to_vec(&resp).expect("JSON serialization cannot fail")
}

/// Decode a poll response.
pub fn decode_poll_response(data: &[u8]) -> Option<Vec<(u32, Vec<u8>)>> {
    let resp: PollResponse = serde_json::from_slice(data).ok()?;
    let mut items = Vec::with_capacity(resp.items.len());
    for item in &resp.items {
        let payload = base64_decode(&item.d)?;
        items.push((item.s, payload));
    }
    Some(items)
}

/// Encode a session init request.
pub fn encode_session_init(client_id_hex: &str, auth_token_hex: &str, timestamp: u64) -> Vec<u8> {
    let req = SessionInitRequest {
        v: 1,
        t: timestamp,
        c: client_id_hex.to_string(),
        a: auth_token_hex.to_string(),
        p: random_padding(64),
    };
    serde_json::to_vec(&req).expect("JSON serialization cannot fail")
}

/// Encode an error response.
pub fn encode_error(message: &str, code: u16) -> Vec<u8> {
    let resp = ErrorResponse {
        error: message.to_string(),
        code,
    };
    serde_json::to_vec(&resp).expect("JSON serialization cannot fail")
}

// --- Binary Encoding ---

/// Binary upload frame: [seq:4 LE][payload_len:4 LE][payload][padding:0-64 random bytes]
fn encode_binary_upload(seq: u32, payload: &[u8]) -> Vec<u8> {
    let mut rng = rand::thread_rng();
    let padding_len = rng.gen_range(0..=64usize);
    let mut buf = Vec::with_capacity(8 + payload.len() + padding_len);
    buf.extend_from_slice(&seq.to_le_bytes());
    buf.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    buf.extend_from_slice(payload);
    // Random padding
    for _ in 0..padding_len {
        buf.push(rng.gen());
    }
    buf
}

/// Decode binary frame: [seq:4 LE][payload_len:4 LE][payload][optional padding...]
fn decode_binary_frame(data: &[u8]) -> Option<(u32, Vec<u8>)> {
    if data.len() < 8 {
        return None;
    }
    let seq = u32::from_le_bytes(data[0..4].try_into().ok()?);
    let payload_len = u32::from_le_bytes(data[4..8].try_into().ok()?) as usize;
    if data.len() < 8 + payload_len {
        return None;
    }
    let payload = data[8..8 + payload_len].to_vec();
    Some((seq, payload))
}

/// Binary download frame: [seq:4 LE][payload_len:4 LE][payload]
fn encode_binary_download(seq: u32, payload: &[u8]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(8 + payload.len());
    buf.extend_from_slice(&seq.to_le_bytes());
    buf.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    buf.extend_from_slice(payload);
    buf
}

// --- Base64 helpers ---

fn base64_encode(data: &[u8]) -> String {
    use std::io::Write;
    let mut buf = Vec::with_capacity(data.len() * 4 / 3 + 4);
    {
        let mut encoder = Base64Encoder::new(&mut buf);
        encoder.write_all(data).unwrap();
        encoder.finish();
    }
    // Safety: base64 produces ASCII
    unsafe { String::from_utf8_unchecked(buf) }
}

fn base64_decode(s: &str) -> Option<Vec<u8>> {
    base64_decode_bytes(s.as_bytes())
}

fn base64_decode_bytes(input: &[u8]) -> Option<Vec<u8>> {
    let mut output = Vec::with_capacity(input.len() * 3 / 4 + 3);
    let mut buf: u32 = 0;
    let mut bits: u32 = 0;

    for &byte in input {
        let val = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' | b'\n' | b'\r' | b' ' => continue,
            _ => return None,
        };
        buf = (buf << 6) | val as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            output.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }

    Some(output)
}

/// Simple base64 encoder (no external dependency needed).
struct Base64Encoder<'a> {
    output: &'a mut Vec<u8>,
    buf: u32,
    bits: u32,
}

const B64_TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

impl<'a> Base64Encoder<'a> {
    fn new(output: &'a mut Vec<u8>) -> Self {
        Self {
            output,
            buf: 0,
            bits: 0,
        }
    }

    fn finish(mut self) {
        if self.bits > 0 {
            self.buf <<= 6 - self.bits;
            self.output.push(B64_TABLE[(self.buf & 0x3F) as usize]);
            match self.bits {
                2 => {
                    self.output.push(b'=');
                    self.output.push(b'=');
                }
                4 => {
                    self.output.push(b'=');
                }
                _ => {}
            }
        }
    }
}

impl<'a> std::io::Write for Base64Encoder<'a> {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        for &byte in data {
            self.buf = (self.buf << 8) | byte as u32;
            self.bits += 8;
            while self.bits >= 6 {
                self.bits -= 6;
                self.output
                    .push(B64_TABLE[((self.buf >> self.bits) & 0x3F) as usize]);
            }
        }
        Ok(data.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base64_roundtrip() {
        let data = b"Hello, XPorta! This is a test payload.";
        let encoded = base64_encode(data);
        let decoded = base64_decode(&encoded).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn test_base64_empty() {
        let encoded = base64_encode(b"");
        let decoded = base64_decode(&encoded).unwrap();
        assert!(decoded.is_empty());
    }

    #[test]
    fn test_base64_various_lengths() {
        for len in 0..=20 {
            let data: Vec<u8> = (0..len).map(|i| i as u8).collect();
            let encoded = base64_encode(&data);
            let decoded = base64_decode(&encoded).unwrap();
            assert_eq!(decoded, data, "Failed for length {}", len);
        }
    }

    #[test]
    fn test_json_request_roundtrip() {
        let payload = b"test data for upload";
        let encoded = encode_request(42, payload, XPortaEncoding::Json);
        let (seq, decoded) = decode_request(&encoded, XPortaEncoding::Json).unwrap();
        assert_eq!(seq, 42);
        assert_eq!(decoded, payload);
    }

    #[test]
    fn test_binary_request_roundtrip() {
        let payload = b"binary test data";
        let encoded = encode_request(100, payload, XPortaEncoding::Binary);
        let (seq, decoded) = decode_request(&encoded, XPortaEncoding::Binary).unwrap();
        assert_eq!(seq, 100);
        assert_eq!(decoded, payload);
    }

    #[test]
    fn test_json_response_with_piggyback() {
        let dl_data = b"download piggyback";
        let encoded = encode_response(Some(7), Some(dl_data), XPortaEncoding::Json);
        let (seq, data) = decode_response(&encoded, XPortaEncoding::Json).unwrap();
        assert_eq!(seq, Some(7));
        assert_eq!(data.unwrap(), dl_data);
    }

    #[test]
    fn test_json_response_no_piggyback() {
        let encoded = encode_response(None, None, XPortaEncoding::Json);
        let (seq, data) = decode_response(&encoded, XPortaEncoding::Json).unwrap();
        assert!(seq.is_none());
        assert!(data.is_none());
    }

    #[test]
    fn test_binary_response_roundtrip() {
        let dl_data = b"binary download";
        let encoded = encode_response(Some(3), Some(dl_data), XPortaEncoding::Binary);
        let (seq, data) = decode_response(&encoded, XPortaEncoding::Binary).unwrap();
        assert_eq!(seq, Some(3));
        assert_eq!(data.unwrap(), dl_data);
    }

    #[test]
    fn test_poll_response_roundtrip() {
        let items: Vec<(u32, &[u8])> = vec![(1, b"first"), (2, b"second"), (3, b"third")];
        let encoded = encode_poll_response(&items);
        let decoded = decode_poll_response(&encoded).unwrap();
        assert_eq!(decoded.len(), 3);
        assert_eq!(decoded[0], (1, b"first".to_vec()));
        assert_eq!(decoded[1], (2, b"second".to_vec()));
        assert_eq!(decoded[2], (3, b"third".to_vec()));
    }

    #[test]
    fn test_poll_response_empty() {
        let items: Vec<(u32, &[u8])> = vec![];
        let encoded = encode_poll_response(&items);
        let decoded = decode_poll_response(&encoded).unwrap();
        assert!(decoded.is_empty());
    }

    #[test]
    fn test_session_init_encoding() {
        let data = encode_session_init("abcdef1234", "deadbeef", 1700000000);
        let req: SessionInitRequest = serde_json::from_slice(&data).unwrap();
        assert_eq!(req.v, 1);
        assert_eq!(req.t, 1700000000);
        assert_eq!(req.c, "abcdef1234");
        assert_eq!(req.a, "deadbeef");
    }

    #[test]
    fn test_error_encoding() {
        let data = encode_error("unauthorized", 401);
        let resp: ErrorResponse = serde_json::from_slice(&data).unwrap();
        assert_eq!(resp.error, "unauthorized");
        assert_eq!(resp.code, 401);
    }
}
