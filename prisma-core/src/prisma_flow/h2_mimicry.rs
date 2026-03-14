//! HTTP/2 SETTINGS frame mimicry.
//!
//! After TLS with ALPN "h2", the first HTTP/2 frame (SETTINGS) uniquely
//! identifies the implementation. This module provides browser-matching profiles.

/// The HTTP/2 connection preface that must be sent before any frames.
const H2_CONNECTION_PREFACE: &[u8] = b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n";

/// HTTP/2 frame type for SETTINGS.
const FRAME_TYPE_SETTINGS: u8 = 0x04;

/// HTTP/2 frame type for WINDOW_UPDATE.
const FRAME_TYPE_WINDOW_UPDATE: u8 = 0x08;

/// HTTP/2 settings profile for a specific browser.
#[derive(Debug, Clone)]
pub struct H2Profile {
    /// SETTINGS parameters in order: (id, value).
    pub settings: Vec<(u16, u32)>,
    /// WINDOW_UPDATE increment to send after SETTINGS.
    pub window_update: u32,
}

/// Returns an HTTP/2 profile matching Chrome's SETTINGS frame fingerprint.
///
/// Chrome sends HEADER_TABLE_SIZE, ENABLE_PUSH (disabled), INITIAL_WINDOW_SIZE,
/// and MAX_HEADER_LIST_SIZE, followed by a large WINDOW_UPDATE.
pub fn chrome_h2_profile() -> H2Profile {
    H2Profile {
        settings: vec![
            (0x01, 65536),   // HEADER_TABLE_SIZE
            (0x02, 0),       // ENABLE_PUSH (disabled)
            (0x04, 6291456), // INITIAL_WINDOW_SIZE
            (0x06, 262144),  // MAX_HEADER_LIST_SIZE
        ],
        window_update: 15663105,
    }
}

/// Returns an HTTP/2 profile matching Firefox's SETTINGS frame fingerprint.
///
/// Firefox sends HEADER_TABLE_SIZE, INITIAL_WINDOW_SIZE, and MAX_FRAME_SIZE.
pub fn firefox_h2_profile() -> H2Profile {
    H2Profile {
        settings: vec![
            (0x01, 65536),  // HEADER_TABLE_SIZE
            (0x04, 131072), // INITIAL_WINDOW_SIZE
            (0x05, 16384),  // MAX_FRAME_SIZE
        ],
        window_update: 12517377,
    }
}

/// Returns an HTTP/2 profile matching Safari's SETTINGS frame fingerprint.
///
/// Safari sends a minimal set: INITIAL_WINDOW_SIZE and MAX_CONCURRENT_STREAMS.
pub fn safari_h2_profile() -> H2Profile {
    H2Profile {
        settings: vec![
            (0x04, 4194304), // INITIAL_WINDOW_SIZE
            (0x03, 100),     // MAX_CONCURRENT_STREAMS
        ],
        window_update: 10485760,
    }
}

/// Build a raw HTTP/2 SETTINGS frame from a profile.
///
/// Frame format per RFC 7540 section 4.1 and 6.5:
/// - Frame header: \[length:3\]\[type:1=0x04\]\[flags:1=0x00\]\[stream_id:4=0x00000000\]
/// - Payload: each setting is \[id:2\]\[value:4\] (6 bytes per setting)
pub fn build_h2_settings_frame(profile: &H2Profile) -> Vec<u8> {
    let payload_len = profile.settings.len() * 6;
    let total_len = 9 + payload_len; // 9 byte frame header + payload
    let mut frame = Vec::with_capacity(total_len);

    // Frame header: 3-byte length
    frame.push(((payload_len >> 16) & 0xFF) as u8);
    frame.push(((payload_len >> 8) & 0xFF) as u8);
    frame.push((payload_len & 0xFF) as u8);

    // Type: SETTINGS (0x04)
    frame.push(FRAME_TYPE_SETTINGS);

    // Flags: 0x00 (no ACK)
    frame.push(0x00);

    // Stream ID: 0x00000000 (connection-level)
    frame.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);

    // Settings payload
    for &(id, value) in &profile.settings {
        frame.push((id >> 8) as u8);
        frame.push((id & 0xFF) as u8);
        frame.push(((value >> 24) & 0xFF) as u8);
        frame.push(((value >> 16) & 0xFF) as u8);
        frame.push(((value >> 8) & 0xFF) as u8);
        frame.push((value & 0xFF) as u8);
    }

    frame
}

/// Build a raw HTTP/2 WINDOW_UPDATE frame.
///
/// Frame format per RFC 7540 section 6.9:
/// - Frame header: \[length:3=4\]\[type:1=0x08\]\[flags:1=0x00\]\[stream_id:4=0x00000000\]
/// - Payload: \[increment:4\] (with reserved high bit clear)
pub fn build_h2_window_update_frame(increment: u32) -> Vec<u8> {
    let mut frame = Vec::with_capacity(13);

    // Frame header: 3-byte length = 4
    frame.extend_from_slice(&[0x00, 0x00, 0x04]);

    // Type: WINDOW_UPDATE (0x08)
    frame.push(FRAME_TYPE_WINDOW_UPDATE);

    // Flags: 0x00
    frame.push(0x00);

    // Stream ID: 0x00000000 (connection-level)
    frame.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);

    // Window size increment (31 bits, high bit reserved and must be 0)
    let increment = increment & 0x7FFF_FFFF;
    frame.push(((increment >> 24) & 0xFF) as u8);
    frame.push(((increment >> 16) & 0xFF) as u8);
    frame.push(((increment >> 8) & 0xFF) as u8);
    frame.push((increment & 0xFF) as u8);

    frame
}

/// Build the full HTTP/2 client connection preface followed by SETTINGS and
/// WINDOW_UPDATE frames for the given profile.
///
/// Per RFC 7540 section 3.5, the client connection preface is:
///   "PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n"
/// followed immediately by a SETTINGS frame.
pub fn build_h2_preface_and_settings(profile: &H2Profile) -> Vec<u8> {
    let settings_frame = build_h2_settings_frame(profile);
    let window_update_frame = build_h2_window_update_frame(profile.window_update);

    let total_len = H2_CONNECTION_PREFACE.len() + settings_frame.len() + window_update_frame.len();
    let mut buf = Vec::with_capacity(total_len);

    buf.extend_from_slice(H2_CONNECTION_PREFACE);
    buf.extend_from_slice(&settings_frame);
    buf.extend_from_slice(&window_update_frame);

    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- Profile correctness tests ----

    #[test]
    fn test_chrome_profile_values() {
        let p = chrome_h2_profile();
        assert_eq!(p.settings.len(), 4);
        assert_eq!(p.settings[0], (0x01, 65536));
        assert_eq!(p.settings[1], (0x02, 0));
        assert_eq!(p.settings[2], (0x04, 6291456));
        assert_eq!(p.settings[3], (0x06, 262144));
        assert_eq!(p.window_update, 15663105);
    }

    #[test]
    fn test_firefox_profile_values() {
        let p = firefox_h2_profile();
        assert_eq!(p.settings.len(), 3);
        assert_eq!(p.settings[0], (0x01, 65536));
        assert_eq!(p.settings[1], (0x04, 131072));
        assert_eq!(p.settings[2], (0x05, 16384));
        assert_eq!(p.window_update, 12517377);
    }

    #[test]
    fn test_safari_profile_values() {
        let p = safari_h2_profile();
        assert_eq!(p.settings.len(), 2);
        assert_eq!(p.settings[0], (0x04, 4194304));
        assert_eq!(p.settings[1], (0x03, 100));
        assert_eq!(p.window_update, 10485760);
    }

    // ---- Frame encoding correctness tests ----

    /// Parse a 9-byte HTTP/2 frame header and return (length, type, flags, stream_id).
    fn parse_frame_header(data: &[u8]) -> (usize, u8, u8, u32) {
        assert!(data.len() >= 9, "frame header must be at least 9 bytes");
        let length = ((data[0] as usize) << 16) | ((data[1] as usize) << 8) | (data[2] as usize);
        let frame_type = data[3];
        let flags = data[4];
        let stream_id = ((data[5] as u32) << 24)
            | ((data[6] as u32) << 16)
            | ((data[7] as u32) << 8)
            | (data[8] as u32);
        (length, frame_type, flags, stream_id)
    }

    /// Parse settings payload into Vec<(u16, u32)>.
    fn parse_settings_payload(data: &[u8]) -> Vec<(u16, u32)> {
        assert_eq!(
            data.len() % 6,
            0,
            "settings payload must be a multiple of 6 bytes"
        );
        data.chunks_exact(6)
            .map(|chunk| {
                let id = ((chunk[0] as u16) << 8) | (chunk[1] as u16);
                let value = ((chunk[2] as u32) << 24)
                    | ((chunk[3] as u32) << 16)
                    | ((chunk[4] as u32) << 8)
                    | (chunk[5] as u32);
                (id, value)
            })
            .collect()
    }

    #[test]
    fn test_settings_frame_header() {
        let profile = chrome_h2_profile();
        let frame = build_h2_settings_frame(&profile);

        let (length, frame_type, flags, stream_id) = parse_frame_header(&frame);

        // Payload = 4 settings * 6 bytes = 24
        assert_eq!(length, 24);
        assert_eq!(frame_type, 0x04); // SETTINGS
        assert_eq!(flags, 0x00);
        assert_eq!(stream_id, 0);
        assert_eq!(frame.len(), 9 + 24);
    }

    #[test]
    fn test_settings_frame_payload_chrome() {
        let profile = chrome_h2_profile();
        let frame = build_h2_settings_frame(&profile);
        let payload = &frame[9..];
        let settings = parse_settings_payload(payload);

        assert_eq!(settings, profile.settings);
    }

    #[test]
    fn test_settings_frame_payload_firefox() {
        let profile = firefox_h2_profile();
        let frame = build_h2_settings_frame(&profile);

        let (length, _, _, _) = parse_frame_header(&frame);
        assert_eq!(length, 18); // 3 settings * 6 bytes

        let payload = &frame[9..];
        let settings = parse_settings_payload(payload);
        assert_eq!(settings, profile.settings);
    }

    #[test]
    fn test_settings_frame_payload_safari() {
        let profile = safari_h2_profile();
        let frame = build_h2_settings_frame(&profile);

        let (length, _, _, _) = parse_frame_header(&frame);
        assert_eq!(length, 12); // 2 settings * 6 bytes

        let payload = &frame[9..];
        let settings = parse_settings_payload(payload);
        assert_eq!(settings, profile.settings);
    }

    #[test]
    fn test_settings_frame_empty_profile() {
        let profile = H2Profile {
            settings: vec![],
            window_update: 0,
        };
        let frame = build_h2_settings_frame(&profile);

        let (length, frame_type, flags, stream_id) = parse_frame_header(&frame);
        assert_eq!(length, 0);
        assert_eq!(frame_type, 0x04);
        assert_eq!(flags, 0x00);
        assert_eq!(stream_id, 0);
        assert_eq!(frame.len(), 9);
    }

    #[test]
    fn test_window_update_frame_encoding() {
        let frame = build_h2_window_update_frame(15663105);

        let (length, frame_type, flags, stream_id) = parse_frame_header(&frame);
        assert_eq!(length, 4);
        assert_eq!(frame_type, 0x08); // WINDOW_UPDATE
        assert_eq!(flags, 0x00);
        assert_eq!(stream_id, 0);
        assert_eq!(frame.len(), 13);

        // Parse the increment from payload
        let payload = &frame[9..13];
        let increment = ((payload[0] as u32) << 24)
            | ((payload[1] as u32) << 16)
            | ((payload[2] as u32) << 8)
            | (payload[3] as u32);
        assert_eq!(increment, 15663105);
    }

    #[test]
    fn test_window_update_frame_clears_reserved_bit() {
        // The reserved high bit (bit 31) should be cleared.
        let frame = build_h2_window_update_frame(0xFFFF_FFFF);
        let payload = &frame[9..13];
        // High bit must be 0
        assert_eq!(payload[0] & 0x80, 0x00);

        let increment = ((payload[0] as u32) << 24)
            | ((payload[1] as u32) << 16)
            | ((payload[2] as u32) << 8)
            | (payload[3] as u32);
        assert_eq!(increment, 0x7FFF_FFFF);
    }

    #[test]
    fn test_preface_and_settings_starts_with_preface() {
        let profile = chrome_h2_profile();
        let buf = build_h2_preface_and_settings(&profile);

        let preface = b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n";
        assert!(buf.starts_with(preface));
    }

    #[test]
    fn test_preface_and_settings_contains_settings_then_window_update() {
        let profile = firefox_h2_profile();
        let buf = build_h2_preface_and_settings(&profile);

        let preface_len = b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n".len(); // 24
        let settings_payload_len = profile.settings.len() * 6;
        let settings_frame_len = 9 + settings_payload_len;
        let window_update_frame_len = 13;

        assert_eq!(
            buf.len(),
            preface_len + settings_frame_len + window_update_frame_len
        );

        // Parse the SETTINGS frame after preface
        let settings_header = &buf[preface_len..preface_len + 9];
        let (length, frame_type, _, _) = parse_frame_header(settings_header);
        assert_eq!(frame_type, 0x04);
        assert_eq!(length, settings_payload_len);

        // Parse the WINDOW_UPDATE frame after SETTINGS
        let wu_offset = preface_len + settings_frame_len;
        let wu_header = &buf[wu_offset..wu_offset + 9];
        let (length, frame_type, _, _) = parse_frame_header(wu_header);
        assert_eq!(frame_type, 0x08);
        assert_eq!(length, 4);

        // Verify the window update increment
        let wu_payload = &buf[wu_offset + 9..wu_offset + 13];
        let increment = ((wu_payload[0] as u32) << 24)
            | ((wu_payload[1] as u32) << 16)
            | ((wu_payload[2] as u32) << 8)
            | (wu_payload[3] as u32);
        assert_eq!(increment, profile.window_update);
    }

    #[test]
    fn test_roundtrip_all_profiles() {
        // Verify that encoding then parsing round-trips for all profiles.
        for profile in &[
            chrome_h2_profile(),
            firefox_h2_profile(),
            safari_h2_profile(),
        ] {
            let frame = build_h2_settings_frame(profile);
            let payload = &frame[9..];
            let parsed = parse_settings_payload(payload);
            assert_eq!(parsed, profile.settings);
        }
    }
}
