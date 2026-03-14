//! Raw ClientHello byte construction.
//!
//! Builds a complete TLS ClientHello at the byte level matching a target
//! browser fingerprint. This replaces rustls's auto-generated ClientHello.
//!
//! Wire format reference:
//! ```text
//! TLS Record:    [0x16][0x03][0x01][length:2]
//! Handshake:     [0x01][length:3]
//! ClientHello:   [version:2=0x0303][random:32][session_id_len:1][session_id:32]
//!                [cipher_suites_len:2][cipher_suites:var]
//!                [compression_len:1][compression:1=0x00]
//!                [extensions_len:2][extensions:var]
//! ```

use rand::Rng;

use super::extensions;
use super::grease;

/// Configuration for building a ClientHello.
pub struct ClientHelloConfig {
    /// The server name for the SNI extension.
    pub server_name: String,
    /// TLS cipher suite identifiers in the order they should appear.
    pub cipher_suites: Vec<u16>,
    /// Extension type identifiers in the order they should appear.
    /// Use 0x0015 for padding -- its size will be auto-calculated.
    pub extensions_order: Vec<u16>,
    /// Named groups (elliptic curves) for the supported_groups extension.
    pub supported_groups: Vec<u16>,
    /// Signature algorithm identifiers.
    pub signature_algorithms: Vec<u16>,
    /// ALPN protocol names (e.g., "h2", "http/1.1").
    pub alpn_protocols: Vec<String>,
    /// X25519 public key for the key_share extension (32 bytes).
    pub x25519_pub_key: [u8; 32],
    /// Whether to insert GREASE values in cipher suites, extensions, etc.
    pub include_grease: bool,
    /// Target total ClientHello size in bytes (512 for Chrome).
    /// Padding extension is sized to reach this target. Set to 0 to skip
    /// automatic padding.
    pub padding_target: usize,
    /// Optional content for the padding extension (e.g., PrismaAuth beacon).
    /// If `None`, zeros are used.
    pub padding_content: Option<Vec<u8>>,
    /// Algorithms for the compress_certificate extension (e.g., 0x0002 for brotli).
    pub compress_certificate_algos: Vec<u16>,
    /// Whether to include the signed_certificate_timestamp extension (type 0x0012).
    pub include_sct: bool,
    /// PSK key exchange modes (e.g., 0x01 for psk_dhe_ke).
    pub psk_modes: Vec<u8>,
}

impl Default for ClientHelloConfig {
    /// Default configuration mimicking a Chrome 120+ fingerprint.
    fn default() -> Self {
        Self {
            server_name: String::new(),
            cipher_suites: vec![
                0x1301, // TLS_AES_128_GCM_SHA256
                0x1302, // TLS_AES_256_GCM_SHA384
                0x1303, // TLS_CHACHA20_POLY1305_SHA256
                0xc02b, // TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256
                0xc02f, // TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256
                0xc02c, // TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384
                0xc030, // TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384
                0xcca9, // TLS_ECDHE_ECDSA_WITH_CHACHA20_POLY1305_SHA256
                0xcca8, // TLS_ECDHE_RSA_WITH_CHACHA20_POLY1305_SHA256
                0xc013, // TLS_ECDHE_RSA_WITH_AES_128_CBC_SHA
                0xc014, // TLS_ECDHE_RSA_WITH_AES_256_CBC_SHA
                0x009c, // TLS_RSA_WITH_AES_128_GCM_SHA256
                0x009d, // TLS_RSA_WITH_AES_256_GCM_SHA384
                0x002f, // TLS_RSA_WITH_AES_128_CBC_SHA
                0x0035, // TLS_RSA_WITH_AES_256_CBC_SHA
            ],
            extensions_order: vec![
                0x0000, // server_name
                0x0017, // extended_master_secret
                0xff01, // renegotiation_info
                0x000a, // supported_groups
                0x000b, // ec_point_formats
                0x0023, // session_ticket
                0x0010, // alpn
                0x0005, // status_request
                0x000d, // signature_algorithms
                0x002b, // supported_versions
                0x002d, // psk_key_exchange_modes
                0x0033, // key_share
                0x001b, // compress_certificate
                0x0015, // padding (should be last or near-last)
            ],
            supported_groups: vec![
                0x001d, // x25519
                0x0017, // secp256r1
                0x0018, // secp384r1
            ],
            signature_algorithms: vec![
                0x0403, // ecdsa_secp256r1_sha256
                0x0804, // rsa_pss_rsae_sha256
                0x0401, // rsa_pkcs1_sha256
                0x0503, // ecdsa_secp384r1_sha384
                0x0805, // rsa_pss_rsae_sha384
                0x0501, // rsa_pkcs1_sha384
                0x0806, // rsa_pss_rsae_sha512
                0x0601, // rsa_pkcs1_sha512
            ],
            alpn_protocols: vec!["h2".into(), "http/1.1".into()],
            x25519_pub_key: [0; 32],
            include_grease: true,
            padding_target: 512,
            padding_content: None,
            compress_certificate_algos: vec![0x0002], // brotli
            include_sct: true,
            psk_modes: vec![0x01], // psk_dhe_ke
        }
    }
}

/// Builder that constructs raw ClientHello bytes.
pub struct ClientHelloBuilder;

impl ClientHelloBuilder {
    /// Build a complete TLS ClientHello record.
    ///
    /// Returns the full TLS record bytes:
    /// `record_header(5) + handshake_header(4) + client_hello_body`
    pub fn build(config: &ClientHelloConfig) -> Vec<u8> {
        let body = Self::build_client_hello_body(config);

        // Wrap in Handshake message: type 0x01 (ClientHello) + 3-byte length
        let handshake_len = body.len();
        let mut handshake = Vec::with_capacity(4 + handshake_len);
        handshake.push(0x01); // Handshake type: ClientHello
                              // 3-byte length (big-endian)
        handshake.push(((handshake_len >> 16) & 0xff) as u8);
        handshake.push(((handshake_len >> 8) & 0xff) as u8);
        handshake.push((handshake_len & 0xff) as u8);
        handshake.extend_from_slice(&body);

        // Wrap in TLS record: type 0x16 (Handshake), version 0x0301 (TLS 1.0 compat)
        let record_payload_len = handshake.len();
        let mut record = Vec::with_capacity(5 + record_payload_len);
        record.push(0x16); // Content type: Handshake
        record.push(0x03); // Version major: 3
        record.push(0x01); // Version minor: 1 (TLS 1.0 in record layer)
        record.extend_from_slice(&(record_payload_len as u16).to_be_bytes());
        record.extend_from_slice(&handshake);

        record
    }

    /// Build just the ClientHello body (no record/handshake headers).
    ///
    /// Layout:
    /// ```text
    /// [version:2=0x0303][random:32][session_id_len:1][session_id:32]
    /// [cipher_suites_len:2][cipher_suites:var]
    /// [compression_len:1][compression:1=0x00]
    /// [extensions_len:2][extensions:var]
    /// ```
    pub fn build_client_hello_body(config: &ClientHelloConfig) -> Vec<u8> {
        let mut rng = rand::thread_rng();

        // Generate GREASE values if needed (Chrome uses up to 3-4 distinct values)
        let grease_vals = if config.include_grease {
            grease::distinct_grease_values(4)
        } else {
            vec![]
        };
        let grease_cipher = grease_vals.first().copied().unwrap_or(0);
        let grease_ext = grease_vals.get(1).copied().unwrap_or(0);
        let grease_group = grease_vals.get(2).copied().unwrap_or(0);
        let grease_version = grease_vals.get(3).copied().unwrap_or(0);

        // --- 1. Client version: TLS 1.2 (real version in supported_versions ext) ---
        let mut body = Vec::with_capacity(512);
        body.extend_from_slice(&[0x03, 0x03]); // TLS 1.2

        // --- 2. Client random (32 bytes) ---
        let mut client_random = [0u8; 32];
        rng.fill(&mut client_random);
        body.extend_from_slice(&client_random);

        // --- 3. Legacy session ID (32 bytes for TLS 1.3 compatibility mode) ---
        let mut session_id = [0u8; 32];
        rng.fill(&mut session_id);
        body.push(32); // session_id length
        body.extend_from_slice(&session_id);

        // --- 4. Cipher suites ---
        let mut cipher_bytes = Vec::new();
        if config.include_grease {
            cipher_bytes.extend_from_slice(&grease_cipher.to_be_bytes());
        }
        for &cs in &config.cipher_suites {
            cipher_bytes.extend_from_slice(&cs.to_be_bytes());
        }
        body.extend_from_slice(&(cipher_bytes.len() as u16).to_be_bytes());
        body.extend_from_slice(&cipher_bytes);

        // --- 5. Compression methods: [1, 0x00] (null compression only) ---
        body.push(0x01); // compression methods length
        body.push(0x00); // null compression

        // --- 6. Build extensions (without padding first, to calculate padding size) ---
        let ext_bytes_no_padding =
            Self::build_extensions(config, grease_ext, grease_group, grease_version, None);

        // --- 7. Calculate padding size ---
        // Total ClientHello body so far (without extensions_len field and extensions):
        //   version(2) + random(32) + session_id_len(1) + session_id(32) +
        //   cipher_suites_len(2) + cipher_suites(var) + compression_len(1) + compression(1)
        // Plus: extensions_len(2) + extensions(var)
        // Plus: handshake header(4) + record header(5)
        let overhead = 4 + 5; // handshake(4) + record(5)
        let body_so_far = body.len();
        let total_without_padding = overhead + body_so_far + 2 + ext_bytes_no_padding.len();

        let padding_data =
            if config.padding_target > 0 && total_without_padding < config.padding_target {
                // We need to add a padding extension. The padding extension header is 4 bytes.
                let available = config.padding_target - total_without_padding;
                if available > 4 {
                    let pad_content_len = available - 4; // subtract extension header (type + length)
                    let content = if let Some(ref beacon) = config.padding_content {
                        let mut c = beacon.clone();
                        c.resize(pad_content_len, 0x00);
                        c
                    } else {
                        vec![0x00; pad_content_len]
                    };
                    Some(content)
                } else {
                    // Not enough room for a meaningful padding extension
                    None
                }
            } else {
                // No padding needed or target already exceeded
                if config.extensions_order.contains(&0x0015) {
                    // Padding was requested but not needed for size; include minimal
                    if let Some(ref beacon) = config.padding_content {
                        Some(beacon.clone())
                    } else {
                        None
                    }
                } else {
                    None
                }
            };

        // Rebuild extensions with padding
        let ext_bytes = Self::build_extensions(
            config,
            grease_ext,
            grease_group,
            grease_version,
            padding_data,
        );

        // --- 8. Append extensions ---
        body.extend_from_slice(&(ext_bytes.len() as u16).to_be_bytes());
        body.extend_from_slice(&ext_bytes);

        body
    }

    /// Build all extensions in the configured order.
    ///
    /// `padding_data`: if `Some`, the padding extension (0x0015) will use these
    /// bytes as content. If `None`, the padding extension is omitted.
    fn build_extensions(
        config: &ClientHelloConfig,
        grease_ext: u16,
        grease_group: u16,
        grease_version: u16,
        padding_data: Option<Vec<u8>>,
    ) -> Vec<u8> {
        let mut ext_bytes = Vec::with_capacity(400);

        for &ext_type in &config.extensions_order {
            // Handle GREASE extension placeholders
            if grease::is_grease(ext_type) {
                ext_bytes.extend_from_slice(&extensions::build_grease_extension(ext_type));
                continue;
            }

            match ext_type {
                0x0000 => {
                    // server_name (SNI)
                    if !config.server_name.is_empty() {
                        ext_bytes.extend_from_slice(&extensions::build_sni_extension(
                            &config.server_name,
                        ));
                    }
                }
                0x0005 => {
                    // status_request (OCSP stapling)
                    ext_bytes.extend_from_slice(&extensions::build_status_request_extension());
                }
                0x000a => {
                    // supported_groups
                    ext_bytes.extend_from_slice(&extensions::build_supported_groups_extension(
                        &config.supported_groups,
                        config.include_grease,
                        grease_group,
                    ));
                }
                0x000b => {
                    // ec_point_formats
                    ext_bytes.extend_from_slice(&extensions::build_ec_point_formats_extension());
                }
                0x000d => {
                    // signature_algorithms
                    ext_bytes.extend_from_slice(&extensions::build_signature_algorithms_extension(
                        &config.signature_algorithms,
                    ));
                }
                0x0010 => {
                    // ALPN
                    if !config.alpn_protocols.is_empty() {
                        let protos: Vec<&str> =
                            config.alpn_protocols.iter().map(|s| s.as_str()).collect();
                        ext_bytes.extend_from_slice(&extensions::build_alpn_extension(&protos));
                    }
                }
                0x0012 => {
                    // signed_certificate_timestamp (SCT) - empty extension, just type + zero length
                    if config.include_sct {
                        ext_bytes.extend_from_slice(&0x0012u16.to_be_bytes());
                        ext_bytes.extend_from_slice(&0u16.to_be_bytes());
                    }
                }
                0x0015 => {
                    // padding
                    if let Some(ref data) = padding_data {
                        ext_bytes.extend_from_slice(&extensions::build_padding_extension(data));
                    }
                }
                0x0017 => {
                    // extended_master_secret
                    ext_bytes
                        .extend_from_slice(&extensions::build_extended_master_secret_extension());
                }
                0x001b => {
                    // compress_certificate
                    if !config.compress_certificate_algos.is_empty() {
                        ext_bytes.extend_from_slice(
                            &extensions::build_compress_certificate_extension(
                                &config.compress_certificate_algos,
                            ),
                        );
                    }
                }
                0x0023 => {
                    // session_ticket
                    ext_bytes.extend_from_slice(&extensions::build_session_ticket_extension());
                }
                0x002b => {
                    // supported_versions
                    ext_bytes.extend_from_slice(&extensions::build_supported_versions_extension(
                        config.include_grease,
                        grease_version,
                    ));
                }
                0x002d => {
                    // psk_key_exchange_modes
                    if !config.psk_modes.is_empty() {
                        ext_bytes.extend_from_slice(&extensions::build_psk_modes_extension(
                            &config.psk_modes,
                        ));
                    }
                }
                0x0033 => {
                    // key_share
                    ext_bytes.extend_from_slice(&extensions::build_key_share_extension(
                        &config.x25519_pub_key,
                        &config.supported_groups,
                    ));
                }
                0xff01 => {
                    // renegotiation_info
                    ext_bytes.extend_from_slice(&extensions::build_renegotiation_info_extension());
                }
                other => {
                    // Unknown extension type -- if it's a GREASE value inserted by the
                    // config, build it as a GREASE extension; otherwise skip.
                    if grease::is_grease(other) {
                        ext_bytes.extend_from_slice(&extensions::build_grease_extension(other));
                    }
                    // Silently skip truly unknown extension types
                }
            }
        }

        // If GREASE is enabled and no GREASE extension was already in the order,
        // prepend one (Chrome places a GREASE extension at the front).
        if config.include_grease
            && !config
                .extensions_order
                .iter()
                .any(|&t| grease::is_grease(t))
        {
            let mut with_grease = Vec::with_capacity(5 + ext_bytes.len());
            with_grease.extend_from_slice(&extensions::build_grease_extension(grease_ext));
            with_grease.extend_from_slice(&ext_bytes);
            return with_grease;
        }

        ext_bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> ClientHelloConfig {
        ClientHelloConfig {
            server_name: "example.com".into(),
            include_grease: false,
            padding_target: 0,
            ..ClientHelloConfig::default()
        }
    }

    #[test]
    fn test_build_produces_valid_tls_record() {
        let config = test_config();
        let record = ClientHelloBuilder::build(&config);

        // TLS record header
        assert_eq!(record[0], 0x16, "content type should be Handshake");
        assert_eq!(record[1], 0x03, "version major");
        assert_eq!(record[2], 0x01, "version minor (TLS 1.0 compat)");

        // Record length
        let record_len = u16::from_be_bytes([record[3], record[4]]) as usize;
        assert_eq!(record_len, record.len() - 5);
    }

    #[test]
    fn test_build_handshake_header() {
        let config = test_config();
        let record = ClientHelloBuilder::build(&config);

        // Handshake type at offset 5
        assert_eq!(record[5], 0x01, "handshake type should be ClientHello");

        // 3-byte handshake length
        let hs_len =
            ((record[6] as usize) << 16) | ((record[7] as usize) << 8) | (record[8] as usize);
        assert_eq!(hs_len, record.len() - 9);
    }

    #[test]
    fn test_client_hello_body_structure() {
        let config = test_config();
        let body = ClientHelloBuilder::build_client_hello_body(&config);

        let mut pos = 0;

        // Version: 0x0303
        assert_eq!(&body[pos..pos + 2], &[0x03, 0x03]);
        pos += 2;

        // Client random: 32 bytes (skip content check, it's random)
        pos += 32;

        // Session ID length: 32
        assert_eq!(body[pos], 32);
        pos += 1;

        // Session ID: 32 bytes
        pos += 32;

        // Cipher suites length
        let cs_len = u16::from_be_bytes([body[pos], body[pos + 1]]) as usize;
        pos += 2;
        // Each cipher suite is 2 bytes
        assert_eq!(cs_len % 2, 0);
        assert_eq!(cs_len / 2, config.cipher_suites.len());
        pos += cs_len;

        // Compression methods
        assert_eq!(body[pos], 0x01); // length = 1
        pos += 1;
        assert_eq!(body[pos], 0x00); // null compression
        pos += 1;

        // Extensions length
        let ext_len = u16::from_be_bytes([body[pos], body[pos + 1]]) as usize;
        pos += 2;
        assert_eq!(ext_len, body.len() - pos);
    }

    #[test]
    fn test_cipher_suites_with_grease() {
        let mut config = test_config();
        config.include_grease = true;
        let body = ClientHelloBuilder::build_client_hello_body(&config);

        // Skip to cipher suites: version(2) + random(32) + session_id_len(1) + session_id(32) = 67
        let pos = 67;
        let cs_len = u16::from_be_bytes([body[pos], body[pos + 1]]) as usize;
        // Should have one extra cipher suite (GREASE) compared to config
        assert_eq!(cs_len / 2, config.cipher_suites.len() + 1);

        // First cipher suite should be a GREASE value
        let first_cs = u16::from_be_bytes([body[pos + 2], body[pos + 3]]);
        assert!(
            grease::is_grease(first_cs),
            "first cipher suite should be GREASE"
        );
    }

    #[test]
    fn test_extensions_contain_sni() {
        let config = test_config();
        let record = ClientHelloBuilder::build(&config);

        // Find "example.com" in the record bytes
        let hostname = b"example.com";
        let found = record.windows(hostname.len()).any(|w| w == hostname);
        assert!(found, "SNI hostname not found in ClientHello");
    }

    #[test]
    fn test_padding_reaches_target() {
        let mut config = test_config();
        config.padding_target = 512;
        let record = ClientHelloBuilder::build(&config);

        // The full record should be exactly 512 bytes
        assert_eq!(
            record.len(),
            512,
            "record should be padded to target size; got {}",
            record.len()
        );
    }

    #[test]
    fn test_padding_with_beacon_content() {
        let mut config = test_config();
        config.padding_target = 512;
        config.padding_content = Some(vec![0xDE, 0xAD, 0xBE, 0xEF]);
        let record = ClientHelloBuilder::build(&config);

        assert_eq!(record.len(), 512);

        // The beacon bytes should appear somewhere in the record
        let beacon = &[0xDE, 0xAD, 0xBE, 0xEF];
        let found = record.windows(beacon.len()).any(|w| w == beacon);
        assert!(found, "beacon content not found in padded ClientHello");
    }

    #[test]
    fn test_no_padding_when_target_zero() {
        let mut config = test_config();
        config.padding_target = 0;
        let record = ClientHelloBuilder::build(&config);

        // Should not contain padding extension type 0x0015
        // (scan for the padding extension type in extension area)
        let body = ClientHelloBuilder::build_client_hello_body(&config);

        // Find extensions start
        let mut pos = 2 + 32 + 1 + 32; // version + random + sid_len + sid
        let cs_len = u16::from_be_bytes([body[pos], body[pos + 1]]) as usize;
        pos += 2 + cs_len + 1 + 1; // cs_len field + cs + comp_len + comp
        let _ext_len = u16::from_be_bytes([body[pos], body[pos + 1]]) as usize;
        pos += 2;

        // Walk extensions
        let mut found_padding = false;
        while pos + 4 <= body.len() {
            let ext_type = u16::from_be_bytes([body[pos], body[pos + 1]]);
            let ext_data_len = u16::from_be_bytes([body[pos + 2], body[pos + 3]]) as usize;
            if ext_type == 0x0015 {
                found_padding = true;
            }
            pos += 4 + ext_data_len;
        }
        assert!(
            !found_padding,
            "padding extension should not be present when target is 0"
        );
        // Record should be smaller than 512
        assert!(record.len() < 512);
    }

    #[test]
    fn test_round_trip_parse_extensions() {
        let config = test_config();
        let body = ClientHelloBuilder::build_client_hello_body(&config);

        // Navigate to extensions
        let mut pos = 2 + 32 + 1 + 32;
        let cs_len = u16::from_be_bytes([body[pos], body[pos + 1]]) as usize;
        pos += 2 + cs_len + 1 + 1;
        let ext_total_len = u16::from_be_bytes([body[pos], body[pos + 1]]) as usize;
        pos += 2;

        let ext_end = pos + ext_total_len;
        let mut ext_types = Vec::new();
        while pos + 4 <= ext_end {
            let ext_type = u16::from_be_bytes([body[pos], body[pos + 1]]);
            let ext_data_len = u16::from_be_bytes([body[pos + 2], body[pos + 3]]) as usize;
            ext_types.push(ext_type);
            pos += 4 + ext_data_len;
        }

        // Verify we consumed exactly the right number of bytes
        assert_eq!(pos, ext_end, "extension parsing did not consume all bytes");

        // Check expected extensions are present
        assert!(ext_types.contains(&0x0000), "SNI missing");
        assert!(ext_types.contains(&0x002b), "supported_versions missing");
        assert!(ext_types.contains(&0x0033), "key_share missing");
        assert!(ext_types.contains(&0x000d), "signature_algorithms missing");
    }

    #[test]
    fn test_default_config_builds_successfully() {
        let mut config = ClientHelloConfig::default();
        config.server_name = "test.example.com".into();
        config.x25519_pub_key = [0x42; 32];
        let record = ClientHelloBuilder::build(&config);

        // Should produce a valid non-empty record
        assert!(record.len() > 100);
        assert_eq!(record[0], 0x16);
    }

    #[test]
    fn test_deterministic_structure_random_content() {
        // Two builds with the same config should produce the same structure
        // but different random bytes (client_random, session_id)
        let config = test_config();
        let r1 = ClientHelloBuilder::build(&config);
        let r2 = ClientHelloBuilder::build(&config);

        // Same length
        assert_eq!(r1.len(), r2.len());
        // Same record type and version
        assert_eq!(&r1[0..3], &r2[0..3]);
        // Different random bytes (extremely unlikely to be equal)
        assert_ne!(&r1[14..46], &r2[14..46], "client_random should differ");
    }
}
