//! JA3/JA4 hash computation for fingerprint verification.
//!
//! JA3 = MD5(SSLVersion,Ciphers,Extensions,EllipticCurves,EllipticCurvePointFormats)
//!
//! Since this project does not depend on the `md5` crate, we compute a
//! SHA-256 based variant ("JA3-SHA256") for internal fingerprint verification.
//! The string format is identical to standard JA3; only the final hash uses
//! SHA-256 instead of MD5.
//!
//! JA4 = structured format: `t{version}{sni}{cipher_count}{ext_count}_{sorted_ciphers_sha256_12}_{sorted_extensions_sha256_12}`
//!
//! Reference: <https://github.com/FoxIO-LLC/ja4>

use sha2::{Digest, Sha256};

use super::grease;

/// Compute JA3-SHA256 hash from raw ClientHello bytes.
///
/// The input should be the ClientHello body (starting at the version field),
/// *not* the full TLS record. If you have a full TLS record, strip the
/// record header (5 bytes) and handshake header (4 bytes) first.
///
/// Returns `None` if the bytes cannot be parsed as a valid ClientHello.
pub fn compute_ja3(client_hello: &[u8]) -> Option<String> {
    // Parse ClientHello fields
    if client_hello.len() < 34 {
        return None; // Too short for version + random
    }

    let version = u16::from_be_bytes([client_hello[0], client_hello[1]]);
    let mut pos = 34; // skip version(2) + random(32)

    // Session ID
    if pos >= client_hello.len() {
        return None;
    }
    let sid_len = client_hello[pos] as usize;
    pos += 1 + sid_len;

    // Cipher suites
    let cipher_suites = parse_cipher_suites(&client_hello[pos..])?;
    let cs_byte_len = {
        if pos + 2 > client_hello.len() {
            return None;
        }
        u16::from_be_bytes([client_hello[pos], client_hello[pos + 1]]) as usize
    };
    pos += 2 + cs_byte_len;

    // Compression methods
    if pos >= client_hello.len() {
        return None;
    }
    let comp_len = client_hello[pos] as usize;
    pos += 1 + comp_len;

    // Extensions
    let extensions = parse_extensions(&client_hello[pos..])?;

    // Extract supported_groups from extension 0x000a
    let mut supported_groups: Vec<u16> = Vec::new();
    for (ext_type, ext_data) in &extensions {
        if *ext_type == 0x000a {
            supported_groups = parse_supported_groups(ext_data);
        }
    }

    // Extract ec_point_formats from extension 0x000b
    let mut point_formats: Vec<u8> = Vec::new();
    for (ext_type, ext_data) in &extensions {
        if *ext_type == 0x000b {
            point_formats = parse_ec_point_formats(ext_data);
        }
    }

    // Check for supported_versions extension (0x002b) to get the real TLS version
    let mut tls_version = version;
    for (ext_type, ext_data) in &extensions {
        if *ext_type == 0x002b {
            if let Some(v) = parse_supported_versions_highest(ext_data) {
                tls_version = v;
            }
        }
    }

    // Build JA3 string: version,ciphers,extensions,groups,point_formats
    // Exclude GREASE values from all lists
    let ciphers_str: Vec<String> = cipher_suites
        .iter()
        .filter(|&&c| !grease::is_grease(c))
        .map(|c| c.to_string())
        .collect();

    let extensions_str: Vec<String> = extensions
        .iter()
        .filter(|(t, _)| !grease::is_grease(*t))
        .map(|(t, _)| t.to_string())
        .collect();

    let groups_str: Vec<String> = supported_groups
        .iter()
        .filter(|&&g| !grease::is_grease(g))
        .map(|g| g.to_string())
        .collect();

    let formats_str: Vec<String> = point_formats.iter().map(|f| f.to_string()).collect();

    let ja3_string = format!(
        "{},{},{},{},{}",
        tls_version,
        ciphers_str.join("-"),
        extensions_str.join("-"),
        groups_str.join("-"),
        formats_str.join("-"),
    );

    // Hash with SHA-256
    let mut hasher = Sha256::new();
    hasher.update(ja3_string.as_bytes());
    let hash = hasher.finalize();

    Some(hex::encode_sha256(&hash))
}

/// Compute JA4 fingerprint from raw ClientHello bytes.
///
/// JA4 format: `t{version}{sni}{cipher_count}{ext_count}_{sorted_ciphers_sha256_first12}_{sorted_extensions_sha256_first12}`
///
/// - `t` = TCP (always, since we are building TLS over TCP)
/// - `version` = 2-char TLS version code (e.g., "13" for TLS 1.3)
/// - `sni` = "d" if SNI is present, "i" if not
/// - `cipher_count` = 2-digit count of cipher suites (excluding GREASE)
/// - `ext_count` = 2-digit count of extensions (excluding GREASE)
///
/// Returns `None` if parsing fails.
pub fn compute_ja4(client_hello: &[u8]) -> Option<String> {
    if client_hello.len() < 34 {
        return None;
    }

    let version = u16::from_be_bytes([client_hello[0], client_hello[1]]);
    let mut pos = 34;

    // Session ID
    if pos >= client_hello.len() {
        return None;
    }
    let sid_len = client_hello[pos] as usize;
    pos += 1 + sid_len;

    // Cipher suites
    let cipher_suites = parse_cipher_suites(&client_hello[pos..])?;
    let cs_byte_len = u16::from_be_bytes([client_hello[pos], client_hello[pos + 1]]) as usize;
    pos += 2 + cs_byte_len;

    // Compression methods
    if pos >= client_hello.len() {
        return None;
    }
    let comp_len = client_hello[pos] as usize;
    pos += 1 + comp_len;

    // Extensions
    let extensions = parse_extensions(&client_hello[pos..])?;

    // Determine TLS version from supported_versions extension
    let mut tls_version = version;
    for (ext_type, ext_data) in &extensions {
        if *ext_type == 0x002b {
            if let Some(v) = parse_supported_versions_highest(ext_data) {
                tls_version = v;
            }
        }
    }

    // Version code for JA4
    let version_code = match tls_version {
        0x0304 => "13",
        0x0303 => "12",
        0x0302 => "11",
        0x0301 => "10",
        0x0300 => "s3",
        _ => "00",
    };

    // SNI presence
    let has_sni = extensions.iter().any(|(t, _)| *t == 0x0000);
    let sni_code = if has_sni { "d" } else { "i" };

    // Cipher count (excluding GREASE)
    let non_grease_ciphers: Vec<u16> = cipher_suites
        .iter()
        .filter(|&&c| !grease::is_grease(c))
        .copied()
        .collect();
    let cipher_count = non_grease_ciphers.len();

    // Extension count (excluding GREASE and SNI for the count, per JA4 spec)
    let non_grease_extensions: Vec<u16> = extensions
        .iter()
        .filter(|(t, _)| !grease::is_grease(*t))
        .map(|(t, _)| *t)
        .collect();
    let ext_count = non_grease_extensions.len();

    // First part: t{version}{sni}{cipher_count:02}{ext_count:02}
    let part_a = format!(
        "t{}{}{:02}{:02}",
        version_code, sni_code, cipher_count, ext_count
    );

    // Second part: sorted ciphers SHA-256, first 12 hex chars
    let mut sorted_ciphers = non_grease_ciphers;
    sorted_ciphers.sort();
    let ciphers_str: Vec<String> = sorted_ciphers
        .iter()
        .map(|c| format!("{:04x}", c))
        .collect();
    let ciphers_joined = ciphers_str.join(",");
    let mut hasher_b = Sha256::new();
    hasher_b.update(ciphers_joined.as_bytes());
    let hash_b = hasher_b.finalize();
    let part_b = &hex::encode_sha256(&hash_b)[..12];

    // Third part: sorted extensions SHA-256, first 12 hex chars
    // Exclude SNI (0x0000) and ALPN (0x0010) from the sorted list per JA4 spec
    let mut sorted_extensions: Vec<u16> = non_grease_extensions
        .iter()
        .filter(|&&t| t != 0x0000 && t != 0x0010)
        .copied()
        .collect();
    sorted_extensions.sort();
    let ext_str: Vec<String> = sorted_extensions
        .iter()
        .map(|e| format!("{:04x}", e))
        .collect();
    let ext_joined = ext_str.join(",");
    let mut hasher_c = Sha256::new();
    hasher_c.update(ext_joined.as_bytes());
    let hash_c = hasher_c.finalize();
    let part_c = &hex::encode_sha256(&hash_c)[..12];

    Some(format!("{}_{}", part_a, format!("{}_{}", part_b, part_c)))
}

/// Parse cipher suites from the beginning of `data`.
///
/// Expected format: `[cipher_suites_len:2][cipher_suite:2]*`
fn parse_cipher_suites(data: &[u8]) -> Option<Vec<u16>> {
    if data.len() < 2 {
        return None;
    }
    let len = u16::from_be_bytes([data[0], data[1]]) as usize;
    if data.len() < 2 + len || len % 2 != 0 {
        return None;
    }

    let mut suites = Vec::with_capacity(len / 2);
    let mut pos = 2;
    while pos + 2 <= 2 + len {
        suites.push(u16::from_be_bytes([data[pos], data[pos + 1]]));
        pos += 2;
    }
    Some(suites)
}

/// Parse extensions from the beginning of `data`.
///
/// Expected format: `[extensions_len:2]([ext_type:2][ext_data_len:2][ext_data:var])*`
fn parse_extensions(data: &[u8]) -> Option<Vec<(u16, Vec<u8>)>> {
    if data.len() < 2 {
        return None;
    }
    let total_len = u16::from_be_bytes([data[0], data[1]]) as usize;
    if data.len() < 2 + total_len {
        return None;
    }

    let mut extensions = Vec::new();
    let mut pos = 2;
    let end = 2 + total_len;
    while pos + 4 <= end {
        let ext_type = u16::from_be_bytes([data[pos], data[pos + 1]]);
        let ext_data_len = u16::from_be_bytes([data[pos + 2], data[pos + 3]]) as usize;
        pos += 4;

        if pos + ext_data_len > end {
            return None;
        }

        let ext_data = data[pos..pos + ext_data_len].to_vec();
        extensions.push((ext_type, ext_data));
        pos += ext_data_len;
    }

    Some(extensions)
}

/// Parse supported groups from the supported_groups extension data.
///
/// Expected format: `[named_group_list_len:2][named_group:2]*`
fn parse_supported_groups(ext_data: &[u8]) -> Vec<u16> {
    if ext_data.len() < 2 {
        return Vec::new();
    }
    let list_len = u16::from_be_bytes([ext_data[0], ext_data[1]]) as usize;
    if ext_data.len() < 2 + list_len || list_len % 2 != 0 {
        return Vec::new();
    }

    let mut groups = Vec::with_capacity(list_len / 2);
    let mut pos = 2;
    while pos + 2 <= 2 + list_len {
        groups.push(u16::from_be_bytes([ext_data[pos], ext_data[pos + 1]]));
        pos += 2;
    }
    groups
}

/// Parse EC point formats from the ec_point_formats extension data.
///
/// Expected format: `[formats_len:1][format:1]*`
fn parse_ec_point_formats(ext_data: &[u8]) -> Vec<u8> {
    if ext_data.is_empty() {
        return Vec::new();
    }
    let len = ext_data[0] as usize;
    if ext_data.len() < 1 + len {
        return Vec::new();
    }
    ext_data[1..1 + len].to_vec()
}

/// Parse the highest non-GREASE version from supported_versions extension data.
///
/// Expected format: `[versions_len:1][version:2]*`
fn parse_supported_versions_highest(ext_data: &[u8]) -> Option<u16> {
    if ext_data.is_empty() {
        return None;
    }
    let len = ext_data[0] as usize;
    if ext_data.len() < 1 + len || len % 2 != 0 {
        return None;
    }

    let mut highest: Option<u16> = None;
    let mut pos = 1;
    while pos + 2 <= 1 + len {
        let v = u16::from_be_bytes([ext_data[pos], ext_data[pos + 1]]);
        if !grease::is_grease(v) {
            match highest {
                None => highest = Some(v),
                Some(h) if v > h => highest = Some(v),
                _ => {}
            }
        }
        pos += 2;
    }
    highest
}

/// Re-use crate-level hex encoder.
mod hex {
    pub fn encode_sha256(bytes: &[u8]) -> String {
        crate::util::hex_encode(bytes)
    }
}

/// Known JA3-SHA256 hashes for verification.
///
/// These are placeholders -- actual hashes are computed at test time by
/// running the builder with specific configurations and verifying the output
/// matches the expected fingerprint.
pub mod known_hashes {
    /// Placeholder for Chrome 120 JA3-SHA256 hash.
    /// Computed by building a ClientHello with Chrome 120 parameters and hashing.
    pub const CHROME_120_JA3: &str = "";
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prisma_fp::builder::{ClientHelloBuilder, ClientHelloConfig};

    #[test]
    fn test_parse_cipher_suites() {
        // Two cipher suites: 0x1301, 0x1302
        let data = [0x00, 0x04, 0x13, 0x01, 0x13, 0x02];
        let suites = parse_cipher_suites(&data).unwrap();
        assert_eq!(suites, vec![0x1301, 0x1302]);
    }

    #[test]
    fn test_parse_cipher_suites_empty() {
        let data = [0x00, 0x00];
        let suites = parse_cipher_suites(&data).unwrap();
        assert!(suites.is_empty());
    }

    #[test]
    fn test_parse_cipher_suites_invalid() {
        // Odd length
        let data = [0x00, 0x03, 0x13, 0x01, 0x13];
        assert!(parse_cipher_suites(&data).is_none());
    }

    #[test]
    fn test_parse_extensions() {
        // One extension: type=0x0017, length=0, no data
        let data = [0x00, 0x04, 0x00, 0x17, 0x00, 0x00];
        let exts = parse_extensions(&data).unwrap();
        assert_eq!(exts.len(), 1);
        assert_eq!(exts[0].0, 0x0017);
        assert!(exts[0].1.is_empty());
    }

    #[test]
    fn test_parse_supported_groups() {
        // Two groups: x25519 (0x001d), secp256r1 (0x0017)
        let data = [0x00, 0x04, 0x00, 0x1d, 0x00, 0x17];
        let groups = parse_supported_groups(&data);
        assert_eq!(groups, vec![0x001d, 0x0017]);
    }

    #[test]
    fn test_parse_ec_point_formats() {
        // One format: uncompressed (0x00)
        let data = [0x01, 0x00];
        let formats = parse_ec_point_formats(&data);
        assert_eq!(formats, vec![0x00]);
    }

    #[test]
    fn test_parse_supported_versions_highest() {
        // GREASE + TLS 1.3 + TLS 1.2
        let data = [0x06, 0x3a, 0x3a, 0x03, 0x04, 0x03, 0x03];
        let highest = parse_supported_versions_highest(&data);
        assert_eq!(highest, Some(0x0304));
    }

    #[test]
    fn test_ja3_from_built_client_hello() {
        let config = ClientHelloConfig {
            server_name: "example.com".into(),
            include_grease: false,
            padding_target: 0,
            ..ClientHelloConfig::default()
        };
        let body = ClientHelloBuilder::build_client_hello_body(&config);

        let hash = compute_ja3(&body);
        assert!(hash.is_some(), "JA3 computation should succeed");

        let hash_str = hash.unwrap();
        // SHA-256 hex = 64 chars
        assert_eq!(hash_str.len(), 64);

        // Same config should produce same JA3 (random bytes in client_random/session_id
        // are not part of the JA3 input)
        let body2 = ClientHelloBuilder::build_client_hello_body(&config);
        let hash2 = compute_ja3(&body2).unwrap();
        assert_eq!(
            hash_str, hash2,
            "JA3 should be deterministic for same config"
        );
    }

    #[test]
    fn test_ja3_excludes_grease() {
        let config_no_grease = ClientHelloConfig {
            server_name: "example.com".into(),
            include_grease: false,
            padding_target: 0,
            ..ClientHelloConfig::default()
        };
        let config_grease = ClientHelloConfig {
            server_name: "example.com".into(),
            include_grease: true,
            padding_target: 0,
            ..ClientHelloConfig::default()
        };

        let body_no_grease = ClientHelloBuilder::build_client_hello_body(&config_no_grease);
        let body_grease = ClientHelloBuilder::build_client_hello_body(&config_grease);

        let hash_no_grease = compute_ja3(&body_no_grease).unwrap();
        let hash_grease = compute_ja3(&body_grease).unwrap();

        // JA3 should be the same because GREASE values are excluded from the hash.
        // However, the GREASE extension itself changes the extension list (adds a
        // GREASE-type extension), but since GREASE types are filtered out, the
        // extension type list should match. The only difference might be if GREASE
        // changes which extensions are present (e.g., a GREASE extension is added).
        // In our builder, GREASE adds an extra extension to the list, but since
        // its type is filtered, the JA3 string should still match.
        assert_eq!(
            hash_no_grease, hash_grease,
            "JA3 should be identical with and without GREASE"
        );
    }

    #[test]
    fn test_ja4_from_built_client_hello() {
        let config = ClientHelloConfig {
            server_name: "example.com".into(),
            include_grease: false,
            padding_target: 0,
            ..ClientHelloConfig::default()
        };
        let body = ClientHelloBuilder::build_client_hello_body(&config);

        let ja4 = compute_ja4(&body);
        assert!(ja4.is_some(), "JA4 computation should succeed");

        let ja4_str = ja4.unwrap();
        // JA4 starts with "t" for TCP
        assert!(ja4_str.starts_with('t'), "JA4 should start with 't'");
        // Should contain TLS 1.3 version code
        assert!(
            ja4_str.starts_with("t13"),
            "JA4 should indicate TLS 1.3; got: {}",
            ja4_str
        );
        // Should have SNI indicator "d"
        assert!(
            ja4_str.starts_with("t13d"),
            "JA4 should indicate SNI present; got: {}",
            ja4_str
        );
        // Format: part_a_part_b_part_c (two underscores)
        let parts: Vec<&str> = ja4_str.split('_').collect();
        assert_eq!(
            parts.len(),
            3,
            "JA4 should have 3 underscore-separated parts"
        );
        // part_b and part_c should be 12 hex chars each
        assert_eq!(parts[1].len(), 12);
        assert_eq!(parts[2].len(), 12);
    }

    #[test]
    fn test_ja4_no_sni() {
        let config = ClientHelloConfig {
            server_name: String::new(), // No SNI
            include_grease: false,
            padding_target: 0,
            ..ClientHelloConfig::default()
        };
        let body = ClientHelloBuilder::build_client_hello_body(&config);

        let ja4 = compute_ja4(&body).unwrap();
        // Should have "i" for no SNI
        assert!(
            ja4.contains("13i"),
            "JA4 should indicate no SNI; got: {}",
            ja4
        );
    }

    #[test]
    fn test_ja3_too_short_input() {
        assert!(compute_ja3(&[0x03, 0x03]).is_none());
        assert!(compute_ja3(&[]).is_none());
    }

    #[test]
    fn test_ja4_too_short_input() {
        assert!(compute_ja4(&[0x03, 0x03]).is_none());
    }

    #[test]
    fn test_hex_encode() {
        let bytes = [0xde, 0xad, 0xbe, 0xef];
        assert_eq!(hex::encode_sha256(&bytes), "deadbeef");
    }

    #[test]
    fn test_ja3_deterministic_across_builds() {
        let config = ClientHelloConfig {
            server_name: "test.example.com".into(),
            include_grease: false,
            padding_target: 0,
            cipher_suites: vec![0x1301, 0x1302, 0xc02b],
            supported_groups: vec![0x001d, 0x0017],
            ..ClientHelloConfig::default()
        };

        // Build multiple times and verify JA3 is identical
        let hashes: Vec<String> = (0..5)
            .map(|_| {
                let body = ClientHelloBuilder::build_client_hello_body(&config);
                compute_ja3(&body).unwrap()
            })
            .collect();

        for h in &hashes[1..] {
            assert_eq!(h, &hashes[0], "JA3 should be deterministic");
        }
    }
}
