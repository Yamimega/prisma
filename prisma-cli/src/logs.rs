use anyhow::Result;
use futures_util::{SinkExt, StreamExt};

use crate::api_client::ApiClient;

pub async fn stream(client: &ApiClient, level: Option<&str>, lines: Option<usize>) -> Result<()> {
    let ws_url = client.ws_url("/api/ws/logs");

    // Build TLS connector that skips cert verification
    let tls_connector = build_tls_connector();

    let connector = tokio_tungstenite::Connector::Rustls(std::sync::Arc::new(tls_connector));

    let (mut ws, _) =
        tokio_tungstenite::connect_async_tls_with_config(&ws_url, None, false, Some(connector))
            .await
            .map_err(|e| anyhow::anyhow!("WebSocket connection failed: {}", e))?;

    // Send filter if level is specified
    if let Some(lvl) = level {
        let filter = serde_json::json!({ "level": lvl, "target": "" });
        ws.send(tokio_tungstenite::tungstenite::Message::Text(
            filter.to_string().into(),
        ))
        .await?;
    }

    let mut count = 0usize;

    while let Some(msg) = ws.next().await {
        match msg {
            Ok(tokio_tungstenite::tungstenite::Message::Text(text)) => {
                if let Ok(entry) = serde_json::from_str::<serde_json::Value>(&text) {
                    print_log_entry(&entry);
                    count += 1;
                    if let Some(max) = lines {
                        if count >= max {
                            break;
                        }
                    }
                }
            }
            Ok(tokio_tungstenite::tungstenite::Message::Close(_)) => break,
            Err(e) => {
                eprintln!("WebSocket error: {}", e);
                break;
            }
            _ => {}
        }
    }

    Ok(())
}

fn print_log_entry(entry: &serde_json::Value) {
    let level = entry["level"].as_str().unwrap_or("?");
    let timestamp = entry["timestamp"]
        .as_str()
        .unwrap_or("")
        .chars()
        .take(19)
        .collect::<String>();
    let target = entry["target"].as_str().unwrap_or("");
    let message = entry["message"].as_str().unwrap_or("");

    let color = match level {
        "ERROR" => "\x1b[31m",
        "WARN" => "\x1b[33m",
        "INFO" => "\x1b[32m",
        "DEBUG" => "\x1b[36m",
        "TRACE" => "\x1b[90m",
        _ => "",
    };

    println!(
        "{} {}{:<5}\x1b[0m {} {}",
        timestamp, color, level, target, message
    );
}

fn build_tls_connector() -> rustls::ClientConfig {
    rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(std::sync::Arc::new(NoVerifier))
        .with_no_client_auth()
}

#[derive(Debug)]
struct NoVerifier;

impl rustls::client::danger::ServerCertVerifier for NoVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> std::result::Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::RSA_PKCS1_SHA256,
            rustls::SignatureScheme::RSA_PKCS1_SHA384,
            rustls::SignatureScheme::RSA_PKCS1_SHA512,
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
            rustls::SignatureScheme::ECDSA_NISTP521_SHA512,
            rustls::SignatureScheme::RSA_PSS_SHA256,
            rustls::SignatureScheme::RSA_PSS_SHA384,
            rustls::SignatureScheme::RSA_PSS_SHA512,
            rustls::SignatureScheme::ED25519,
            rustls::SignatureScheme::ED448,
        ]
    }
}
