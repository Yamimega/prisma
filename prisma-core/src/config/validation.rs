use crate::error::ConfigError;
use crate::util::hex_decode;

use super::client::ClientConfig;
use super::server::ServerConfig;

pub fn validate_server_config(config: &ServerConfig) -> Result<(), ConfigError> {
    if config.listen_addr.is_empty() {
        return Err(ConfigError::ValidationFailed(
            "listen_addr must not be empty".into(),
        ));
    }

    if config.authorized_clients.is_empty() {
        return Err(ConfigError::ValidationFailed(
            "at least one authorized client must be configured".into(),
        ));
    }

    for (i, client) in config.authorized_clients.iter().enumerate() {
        if client.id.is_empty() {
            return Err(ConfigError::ValidationFailed(format!(
                "authorized_clients[{}].id must not be empty",
                i
            )));
        }
        if client.auth_secret.is_empty() {
            return Err(ConfigError::ValidationFailed(format!(
                "authorized_clients[{}].auth_secret must not be empty",
                i
            )));
        }
        if hex_decode(&client.auth_secret).is_none() {
            return Err(ConfigError::ValidationFailed(format!(
                "authorized_clients[{}].auth_secret must be valid hex",
                i
            )));
        }
    }

    validate_logging_level(&config.logging.level)?;
    validate_logging_format(&config.logging.format)?;

    Ok(())
}

pub fn validate_client_config(config: &ClientConfig) -> Result<(), ConfigError> {
    if config.socks5_listen_addr.is_empty() {
        return Err(ConfigError::ValidationFailed(
            "socks5_listen_addr must not be empty".into(),
        ));
    }

    if config.server_addr.is_empty() {
        return Err(ConfigError::ValidationFailed(
            "server_addr must not be empty".into(),
        ));
    }

    if config.identity.client_id.is_empty() {
        return Err(ConfigError::ValidationFailed(
            "identity.client_id must not be empty".into(),
        ));
    }

    if hex_decode(&config.identity.auth_secret).is_none() {
        return Err(ConfigError::ValidationFailed(
            "identity.auth_secret must be valid hex".into(),
        ));
    }

    let valid_ciphers = ["chacha20-poly1305", "aes-256-gcm"];
    if !valid_ciphers.contains(&config.cipher_suite.as_str()) {
        return Err(ConfigError::ValidationFailed(format!(
            "cipher_suite must be one of: {:?}",
            valid_ciphers
        )));
    }

    let valid_transports = ["quic", "tcp"];
    if !valid_transports.contains(&config.transport.as_str()) {
        return Err(ConfigError::ValidationFailed(format!(
            "transport must be one of: {:?}",
            valid_transports
        )));
    }

    validate_logging_level(&config.logging.level)?;
    validate_logging_format(&config.logging.format)?;

    Ok(())
}

fn validate_logging_level(level: &str) -> Result<(), ConfigError> {
    let valid = ["trace", "debug", "info", "warn", "error"];
    if !valid.contains(&level) {
        return Err(ConfigError::ValidationFailed(format!(
            "logging.level must be one of: {:?}",
            valid
        )));
    }
    Ok(())
}

fn validate_logging_format(format: &str) -> Result<(), ConfigError> {
    let valid = ["pretty", "json"];
    if !valid.contains(&format) {
        return Err(ConfigError::ValidationFailed(format!(
            "logging.format must be one of: {:?}",
            valid
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::util::hex_decode;

    #[test]
    fn test_hex_decode_valid() {
        assert_eq!(hex_decode("deadbeef"), Some(vec![0xde, 0xad, 0xbe, 0xef]));
    }

    #[test]
    fn test_hex_decode_invalid() {
        assert_eq!(hex_decode("xyz"), None);
    }
}
