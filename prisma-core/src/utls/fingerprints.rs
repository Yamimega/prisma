//! Browser ClientHello fingerprint templates.
//!
//! Each template defines the cipher suites, extensions, ALPN, supported groups,
//! and signature algorithms that match a specific real browser's TLS ClientHello.
//! This ensures the JA3/JA4 fingerprint hash matches the target browser.

/// A ClientHello template describing browser-specific TLS parameters.
#[derive(Debug, Clone)]
pub struct ClientHelloTemplate {
    /// Display name for logging.
    pub name: &'static str,
    /// TLS cipher suite identifiers (IANA values) in browser order.
    pub cipher_suites: Vec<u16>,
    /// ALPN protocol strings.
    pub alpn_protocols: Vec<String>,
    /// Supported elliptic curve groups (IANA NamedGroup values).
    pub supported_groups: Vec<u16>,
    /// Signature algorithms (IANA SignatureScheme values).
    pub signature_algorithms: Vec<u16>,
    /// TLS extension IDs in the order they appear in the ClientHello.
    pub extensions_order: Vec<u16>,
    /// Whether to include the GREASE (Generate Random Extensions And Sustain Extensibility) values.
    pub grease: bool,
    /// Padding target size for the ClientHello (0 = no padding).
    /// Chrome pads to 512 bytes when the ClientHello is between 256-511 bytes.
    pub padding_target: u16,
    /// Supported TLS versions.
    pub supported_versions: Vec<u16>,
    /// PSK key exchange modes.
    pub psk_key_exchange_modes: Vec<u8>,
    /// Compress certificate algorithms (e.g., brotli = 2).
    pub compress_certificate_algos: Vec<u16>,
}

/// Chrome 120+ on Windows 10/11 fingerprint template.
///
/// JA3 hash should match real Chrome installations.
/// Based on Chrome's BoringSSL configuration.
pub fn chrome_120() -> ClientHelloTemplate {
    ClientHelloTemplate {
        name: "Chrome 120",
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
        alpn_protocols: vec!["h2".into(), "http/1.1".into()],
        supported_groups: vec![
            0x4588, // GREASE
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
        extensions_order: vec![
            0x0000, // server_name
            0xff01, // extended_master_secret (renegotiation_info)
            0x000a, // supported_groups
            0x000b, // ec_point_formats
            0x0023, // session_ticket
            0x0010, // alpn
            0x0005, // status_request
            0x000d, // signature_algorithms
            0x0012, // signed_certificate_timestamp
            0x002b, // supported_versions
            0x002d, // psk_key_exchange_modes
            0x0033, // key_share
            0x001b, // compress_certificate
            0x0015, // padding
        ],
        grease: true,
        padding_target: 512,
        supported_versions: vec![
            0x0304, // TLS 1.3
            0x0303, // TLS 1.2
        ],
        psk_key_exchange_modes: vec![0x01],       // psk_dhe_ke
        compress_certificate_algos: vec![0x0002], // brotli
    }
}

/// Firefox 121+ on Windows 10/11 fingerprint template.
///
/// Firefox uses NSS for TLS, which has different cipher suite ordering
/// and extension handling than Chrome's BoringSSL.
pub fn firefox_121() -> ClientHelloTemplate {
    ClientHelloTemplate {
        name: "Firefox 121",
        cipher_suites: vec![
            0x1301, // TLS_AES_128_GCM_SHA256
            0x1303, // TLS_CHACHA20_POLY1305_SHA256
            0x1302, // TLS_AES_256_GCM_SHA384
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
            0x00ff, // TLS_EMPTY_RENEGOTIATION_INFO_SCSV
        ],
        alpn_protocols: vec!["h2".into(), "http/1.1".into()],
        supported_groups: vec![
            0x001d, // x25519
            0x0017, // secp256r1
            0x0018, // secp384r1
            0x0100, // ffdhe2048
            0x0101, // ffdhe3072
        ],
        signature_algorithms: vec![
            0x0403, // ecdsa_secp256r1_sha256
            0x0503, // ecdsa_secp384r1_sha384
            0x0603, // ecdsa_secp521r1_sha512
            0x0804, // rsa_pss_rsae_sha256
            0x0805, // rsa_pss_rsae_sha384
            0x0806, // rsa_pss_rsae_sha512
            0x0401, // rsa_pkcs1_sha256
            0x0501, // rsa_pkcs1_sha384
            0x0601, // rsa_pkcs1_sha512
        ],
        extensions_order: vec![
            0x0000, // server_name
            0xff01, // extended_master_secret
            0x000a, // supported_groups
            0x000b, // ec_point_formats
            0x0023, // session_ticket
            0x0010, // alpn
            0x0005, // status_request
            0x0022, // delegated_credentials
            0x000d, // signature_algorithms
            0x002b, // supported_versions
            0x002d, // psk_key_exchange_modes
            0x0033, // key_share
            0x001c, // record_size_limit
        ],
        grease: false, // Firefox does not use GREASE
        padding_target: 0,
        supported_versions: vec![
            0x0304, // TLS 1.3
            0x0303, // TLS 1.2
        ],
        psk_key_exchange_modes: vec![0x01], // psk_dhe_ke
        compress_certificate_algos: vec![],
    }
}

/// Safari 17+ on macOS Sonoma fingerprint template.
///
/// Safari uses Apple's Secure Transport / BoringSSL fork.
pub fn safari_17() -> ClientHelloTemplate {
    ClientHelloTemplate {
        name: "Safari 17",
        cipher_suites: vec![
            0x1301, // TLS_AES_128_GCM_SHA256
            0x1302, // TLS_AES_256_GCM_SHA384
            0x1303, // TLS_CHACHA20_POLY1305_SHA256
            0xc02c, // TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384
            0xc02b, // TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256
            0xc030, // TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384
            0xc02f, // TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256
            0xcca9, // TLS_ECDHE_ECDSA_WITH_CHACHA20_POLY1305_SHA256
            0xcca8, // TLS_ECDHE_RSA_WITH_CHACHA20_POLY1305_SHA256
            0xc024, // TLS_ECDHE_ECDSA_WITH_AES_256_CBC_SHA384
            0xc023, // TLS_ECDHE_ECDSA_WITH_AES_128_CBC_SHA256
            0xc014, // TLS_ECDHE_RSA_WITH_AES_256_CBC_SHA
            0xc013, // TLS_ECDHE_RSA_WITH_AES_128_CBC_SHA
            0x009d, // TLS_RSA_WITH_AES_256_GCM_SHA384
            0x009c, // TLS_RSA_WITH_AES_128_GCM_SHA256
            0x0035, // TLS_RSA_WITH_AES_256_CBC_SHA
            0x002f, // TLS_RSA_WITH_AES_128_CBC_SHA
        ],
        alpn_protocols: vec!["h2".into(), "http/1.1".into()],
        supported_groups: vec![
            0x001d, // x25519
            0x0017, // secp256r1
            0x0018, // secp384r1
            0x0019, // secp521r1
        ],
        signature_algorithms: vec![
            0x0403, // ecdsa_secp256r1_sha256
            0x0503, // ecdsa_secp384r1_sha384
            0x0603, // ecdsa_secp521r1_sha512
            0x0804, // rsa_pss_rsae_sha256
            0x0805, // rsa_pss_rsae_sha384
            0x0806, // rsa_pss_rsae_sha512
            0x0401, // rsa_pkcs1_sha256
            0x0501, // rsa_pkcs1_sha384
            0x0601, // rsa_pkcs1_sha512
        ],
        extensions_order: vec![
            0x0000, // server_name
            0xff01, // extended_master_secret
            0x000a, // supported_groups
            0x000b, // ec_point_formats
            0x0010, // alpn
            0x0005, // status_request
            0x000d, // signature_algorithms
            0x0012, // signed_certificate_timestamp
            0x002b, // supported_versions
            0x002d, // psk_key_exchange_modes
            0x0033, // key_share
        ],
        grease: true,
        padding_target: 0,
        supported_versions: vec![
            0x0304, // TLS 1.3
            0x0303, // TLS 1.2
        ],
        psk_key_exchange_modes: vec![0x01], // psk_dhe_ke
        compress_certificate_algos: vec![],
    }
}

/// Default template (no fingerprint mimicry — uses standard rustls behavior).
pub fn default_template() -> ClientHelloTemplate {
    ClientHelloTemplate {
        name: "Default (rustls)",
        cipher_suites: vec![
            0x1301, // TLS_AES_128_GCM_SHA256
            0x1302, // TLS_AES_256_GCM_SHA384
            0x1303, // TLS_CHACHA20_POLY1305_SHA256
        ],
        alpn_protocols: vec!["h2".into(), "http/1.1".into()],
        supported_groups: vec![
            0x001d, // x25519
            0x0017, // secp256r1
            0x0018, // secp384r1
        ],
        signature_algorithms: vec![
            0x0403, 0x0804, 0x0401, 0x0503, 0x0805, 0x0501, 0x0806, 0x0601,
        ],
        extensions_order: vec![],
        grease: false,
        padding_target: 0,
        supported_versions: vec![0x0304, 0x0303],
        psk_key_exchange_modes: vec![0x01],
        compress_certificate_algos: vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chrome_template_valid() {
        let t = chrome_120();
        assert!(!t.cipher_suites.is_empty());
        assert!(t.cipher_suites.contains(&0x1301)); // Must include TLS 1.3
        assert!(t.grease); // Chrome uses GREASE
        assert_eq!(t.padding_target, 512);
    }

    #[test]
    fn test_firefox_no_grease() {
        let t = firefox_121();
        assert!(!t.grease);
        // Firefox includes renegotiation_info SCSV
        assert!(t.cipher_suites.contains(&0x00ff));
    }

    #[test]
    fn test_safari_has_secp521() {
        let t = safari_17();
        assert!(t.supported_groups.contains(&0x0019)); // secp521r1
    }
}
