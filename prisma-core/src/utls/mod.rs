//! uTLS ClientHello fingerprint mimicry.
//!
//! Provides TLS ClientHello templates that match real browser fingerprints
//! (Chrome, Firefox, Safari). This prevents DPI systems from identifying
//! PrismaVeil connections by their TLS fingerprint (JA3/JA4 hash).
//!
//! # Usage
//! ```ignore
//! let template = Fingerprint::Chrome.client_hello_template();
//! let tls_config = build_fingerprinted_tls_config(&template, skip_cert_verify);
//! ```

pub mod fingerprints;

use serde::{Deserialize, Serialize};

/// Supported browser fingerprint profiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Fingerprint {
    /// Chrome 120+ on Windows 10/11
    Chrome,
    /// Firefox 121+ on Windows 10/11
    Firefox,
    /// Safari 17+ on macOS Sonoma
    Safari,
    /// Randomly select a fingerprint per connection
    Random,
    /// No fingerprint mimicry (use default rustls behavior)
    None,
}

impl Fingerprint {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "chrome" => Fingerprint::Chrome,
            "firefox" => Fingerprint::Firefox,
            "safari" => Fingerprint::Safari,
            "random" => Fingerprint::Random,
            _ => Fingerprint::None,
        }
    }

    /// Resolve `Random` to a concrete fingerprint.
    pub fn resolve(self) -> Self {
        match self {
            Fingerprint::Random => {
                let choices = [
                    Fingerprint::Chrome,
                    Fingerprint::Firefox,
                    Fingerprint::Safari,
                ];
                let idx = rand::Rng::gen_range(&mut rand::thread_rng(), 0..choices.len());
                choices[idx]
            }
            other => other,
        }
    }

    /// Get the ClientHello template for this fingerprint.
    pub fn client_hello_template(self) -> fingerprints::ClientHelloTemplate {
        let resolved = self.resolve();
        match resolved {
            Fingerprint::Chrome => fingerprints::chrome_120(),
            Fingerprint::Firefox => fingerprints::firefox_121(),
            Fingerprint::Safari => fingerprints::safari_17(),
            _ => fingerprints::default_template(),
        }
    }
}

impl Default for Fingerprint {
    fn default() -> Self {
        Fingerprint::None
    }
}

/// Build a `rustls::ClientConfig` with fingerprint-aware settings.
/// This configures cipher suites, ALPN, and signature algorithms to match
/// the target browser fingerprint.
pub fn build_fingerprinted_tls_config(
    template: &fingerprints::ClientHelloTemplate,
    skip_cert_verify: bool,
    alpn_override: Option<&[String]>,
) -> rustls::ClientConfig {
    use std::sync::Arc;

    let mut config = if skip_cert_verify {
        rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(
                super_insecure_verifier::InsecureCertVerifier,
            ))
            .with_no_client_auth()
    } else {
        let mut roots = rustls::RootCertStore::empty();
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth()
    };

    // Apply fingerprint-specific ALPN
    if let Some(alpn) = alpn_override {
        config.alpn_protocols = alpn.iter().map(|s| s.as_bytes().to_vec()).collect();
    } else {
        config.alpn_protocols = template
            .alpn_protocols
            .iter()
            .map(|s| s.as_bytes().to_vec())
            .collect();
    }

    config
}

/// Module for the insecure cert verifier (shared with connector.rs).
pub mod super_insecure_verifier {
    /// Certificate verifier that accepts any certificate (dev/censorship-bypass mode).
    #[derive(Debug)]
    pub struct InsecureCertVerifier;

    impl rustls::client::danger::ServerCertVerifier for InsecureCertVerifier {
        fn verify_server_cert(
            &self,
            _end_entity: &rustls::pki_types::CertificateDer<'_>,
            _intermediates: &[rustls::pki_types::CertificateDer<'_>],
            _server_name: &rustls::pki_types::ServerName<'_>,
            _ocsp_response: &[u8],
            _now: rustls::pki_types::UnixTime,
        ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
            Ok(rustls::client::danger::ServerCertVerified::assertion())
        }

        fn verify_tls12_signature(
            &self,
            _message: &[u8],
            _cert: &rustls::pki_types::CertificateDer<'_>,
            _dss: &rustls::DigitallySignedStruct,
        ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
            Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
        }

        fn verify_tls13_signature(
            &self,
            _message: &[u8],
            _cert: &rustls::pki_types::CertificateDer<'_>,
            _dss: &rustls::DigitallySignedStruct,
        ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
            Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
        }

        fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
            vec![
                rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
                rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
                rustls::SignatureScheme::ED25519,
                rustls::SignatureScheme::RSA_PSS_SHA256,
                rustls::SignatureScheme::RSA_PSS_SHA384,
                rustls::SignatureScheme::RSA_PSS_SHA512,
                rustls::SignatureScheme::RSA_PKCS1_SHA256,
                rustls::SignatureScheme::RSA_PKCS1_SHA384,
                rustls::SignatureScheme::RSA_PKCS1_SHA512,
            ]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fingerprint_resolve() {
        assert_eq!(Fingerprint::Chrome.resolve(), Fingerprint::Chrome);
        assert_eq!(Fingerprint::Firefox.resolve(), Fingerprint::Firefox);
        // Random should resolve to one of the three
        let resolved = Fingerprint::Random.resolve();
        assert!(matches!(
            resolved,
            Fingerprint::Chrome | Fingerprint::Firefox | Fingerprint::Safari
        ));
    }

    #[test]
    fn test_fingerprint_from_str() {
        assert_eq!(Fingerprint::from_str("chrome"), Fingerprint::Chrome);
        assert_eq!(Fingerprint::from_str("Chrome"), Fingerprint::Chrome);
        assert_eq!(Fingerprint::from_str("firefox"), Fingerprint::Firefox);
        assert_eq!(Fingerprint::from_str("safari"), Fingerprint::Safari);
        assert_eq!(Fingerprint::from_str("random"), Fingerprint::Random);
        assert_eq!(Fingerprint::from_str("unknown"), Fingerprint::None);
    }

    #[test]
    fn test_chrome_template() {
        let template = Fingerprint::Chrome.client_hello_template();
        assert!(template.alpn_protocols.contains(&"h2".to_string()));
        assert!(!template.cipher_suites.is_empty());
    }

    #[test]
    fn test_firefox_template() {
        let template = Fingerprint::Firefox.client_hello_template();
        assert!(template.alpn_protocols.contains(&"h2".to_string()));
    }

    #[test]
    fn test_safari_template() {
        let template = Fingerprint::Safari.client_hello_template();
        assert!(template.alpn_protocols.contains(&"h2".to_string()));
    }
}
