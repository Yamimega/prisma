pub mod client;
pub mod server;
pub mod validation;

use crate::error::ConfigError;

/// Load server config from file path with layered overrides:
/// defaults → TOML file → env vars (PRISMA_*)
pub fn load_server_config(path: &str) -> Result<server::ServerConfig, ConfigError> {
    let builder = config::Config::builder()
        .set_default("listen_addr", "0.0.0.0:8443")
        .map_err(|e| ConfigError::Invalid(e.to_string()))?
        .set_default("quic_listen_addr", "0.0.0.0:8443")
        .map_err(|e| ConfigError::Invalid(e.to_string()))?
        .set_default("logging.level", "info")
        .map_err(|e| ConfigError::Invalid(e.to_string()))?
        .set_default("logging.format", "pretty")
        .map_err(|e| ConfigError::Invalid(e.to_string()))?
        .set_default("performance.max_connections", 1024i64)
        .map_err(|e| ConfigError::Invalid(e.to_string()))?
        .set_default("performance.connection_timeout_secs", 300i64)
        .map_err(|e| ConfigError::Invalid(e.to_string()))?
        .add_source(config::File::with_name(path).required(true))
        .add_source(
            config::Environment::with_prefix("PRISMA")
                .separator("_")
                .try_parsing(true),
        );

    let config = builder
        .build()
        .map_err(|e| ConfigError::ParseError(e.to_string()))?;

    let server_config: server::ServerConfig = config
        .try_deserialize()
        .map_err(|e| ConfigError::ParseError(e.to_string()))?;

    validation::validate_server_config(&server_config)?;

    Ok(server_config)
}

/// Load client config from file path with layered overrides.
pub fn load_client_config(path: &str) -> Result<client::ClientConfig, ConfigError> {
    let builder = config::Config::builder()
        .set_default("socks5_listen_addr", "127.0.0.1:1080")
        .map_err(|e| ConfigError::Invalid(e.to_string()))?
        .set_default("cipher_suite", "chacha20-poly1305")
        .map_err(|e| ConfigError::Invalid(e.to_string()))?
        .set_default("logging.level", "info")
        .map_err(|e| ConfigError::Invalid(e.to_string()))?
        .set_default("logging.format", "pretty")
        .map_err(|e| ConfigError::Invalid(e.to_string()))?
        .set_default("transport", "quic")
        .map_err(|e| ConfigError::Invalid(e.to_string()))?
        .set_default("skip_cert_verify", false)
        .map_err(|e| ConfigError::Invalid(e.to_string()))?
        .add_source(config::File::with_name(path).required(true))
        .add_source(
            config::Environment::with_prefix("PRISMA")
                .separator("_")
                .try_parsing(true),
        );

    let config = builder
        .build()
        .map_err(|e| ConfigError::ParseError(e.to_string()))?;

    let client_config: client::ClientConfig = config
        .try_deserialize()
        .map_err(|e| ConfigError::ParseError(e.to_string()))?;

    validation::validate_client_config(&client_config)?;

    Ok(client_config)
}
