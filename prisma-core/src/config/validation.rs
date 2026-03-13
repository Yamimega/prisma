use crate::error::ConfigError;
use crate::util::hex_decode;

use super::client::{ClientConfig, CongestionConfig};
use super::server::ServerConfig;

/// Regex-style check for bandwidth format like "100mbps", "1gbps", "50kbps".
fn is_valid_bandwidth_format(s: &str) -> bool {
    let s = s.to_ascii_lowercase();
    let suffixes = ["kbps", "mbps", "gbps"];
    for suffix in &suffixes {
        if let Some(num_part) = s.strip_suffix(suffix) {
            if !num_part.is_empty() && num_part.chars().all(|c| c.is_ascii_digit() || c == '.') {
                return true;
            }
        }
    }
    false
}

/// Check for quota format like "100GB", "1TB", "500MB".
fn is_valid_quota_format(s: &str) -> bool {
    let s = s.to_ascii_uppercase();
    let suffixes = ["KB", "MB", "GB", "TB"];
    for suffix in &suffixes {
        if let Some(num_part) = s.strip_suffix(suffix) {
            if !num_part.is_empty() && num_part.chars().all(|c| c.is_ascii_digit() || c == '.') {
                return true;
            }
        }
    }
    false
}

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

        // Bandwidth format validation
        if let Some(ref bw) = client.bandwidth_up {
            if !is_valid_bandwidth_format(bw) {
                return Err(ConfigError::ValidationFailed(format!(
                    "authorized_clients[{}].bandwidth_up \"{}\" must match format like \"100mbps\", \"1gbps\", \"50kbps\"",
                    i, bw
                )));
            }
        }
        if let Some(ref bw) = client.bandwidth_down {
            if !is_valid_bandwidth_format(bw) {
                return Err(ConfigError::ValidationFailed(format!(
                    "authorized_clients[{}].bandwidth_down \"{}\" must match format like \"100mbps\", \"1gbps\", \"50kbps\"",
                    i, bw
                )));
            }
        }

        // Quota format validation
        if let Some(ref quota) = client.quota {
            if !is_valid_quota_format(quota) {
                return Err(ConfigError::ValidationFailed(format!(
                    "authorized_clients[{}].quota \"{}\" must match format like \"100GB\", \"1TB\", \"500MB\"",
                    i, quota
                )));
            }
        }

        // Quota period validation
        if let Some(ref period) = client.quota_period {
            let valid_periods = ["daily", "weekly", "monthly"];
            if !valid_periods.contains(&period.as_str()) {
                return Err(ConfigError::ValidationFailed(format!(
                    "authorized_clients[{}].quota_period must be one of: {:?}",
                    i, valid_periods
                )));
            }
        }
    }

    validate_logging_level(&config.logging.level)?;
    validate_logging_format(&config.logging.format)?;

    // CDN validation
    if config.cdn.enabled {
        if config.cdn.tls.is_none() {
            return Err(ConfigError::ValidationFailed(
                "cdn.enabled requires [cdn.tls] config (cert_path and key_path)".into(),
            ));
        }
        if !config.cdn.ws_tunnel_path.starts_with('/') {
            return Err(ConfigError::ValidationFailed(
                "cdn.ws_tunnel_path must start with '/'".into(),
            ));
        }
        if !config.cdn.grpc_tunnel_path.starts_with('/') {
            return Err(ConfigError::ValidationFailed(
                "cdn.grpc_tunnel_path must start with '/'".into(),
            ));
        }
        if !config.cdn.xhttp_upload_path.starts_with('/') {
            return Err(ConfigError::ValidationFailed(
                "cdn.xhttp_upload_path must start with '/'".into(),
            ));
        }
        if !config.cdn.xhttp_download_path.starts_with('/') {
            return Err(ConfigError::ValidationFailed(
                "cdn.xhttp_download_path must start with '/'".into(),
            ));
        }
        if !config.cdn.xhttp_stream_path.starts_with('/') {
            return Err(ConfigError::ValidationFailed(
                "cdn.xhttp_stream_path must start with '/'".into(),
            ));
        }
        if let Some(ref mode) = config.cdn.xhttp_mode {
            let valid = ["packet-up", "stream-up", "stream-one"];
            if !valid.contains(&mode.as_str()) {
                return Err(ConfigError::ValidationFailed(format!(
                    "cdn.xhttp_mode must be one of: {:?}",
                    valid
                )));
            }
        }
    }

    // XPorta validation (within CDN)
    if let Some(ref xporta) = config.cdn.xporta {
        if xporta.enabled {
            if !xporta.session_path.starts_with('/') {
                return Err(ConfigError::ValidationFailed(
                    "cdn.xporta.session_path must start with '/'".into(),
                ));
            }
            if xporta.data_paths.is_empty() {
                return Err(ConfigError::ValidationFailed(
                    "cdn.xporta.data_paths must not be empty".into(),
                ));
            }
            if xporta.poll_paths.is_empty() {
                return Err(ConfigError::ValidationFailed(
                    "cdn.xporta.poll_paths must not be empty".into(),
                ));
            }
            for path in &xporta.data_paths {
                if !path.starts_with('/') {
                    return Err(ConfigError::ValidationFailed(format!(
                        "cdn.xporta.data_paths entry \"{}\" must start with '/'",
                        path
                    )));
                }
            }
            for path in &xporta.poll_paths {
                if !path.starts_with('/') {
                    return Err(ConfigError::ValidationFailed(format!(
                        "cdn.xporta.poll_paths entry \"{}\" must start with '/'",
                        path
                    )));
                }
            }
            for dp in &xporta.data_paths {
                if xporta.poll_paths.contains(dp) {
                    return Err(ConfigError::ValidationFailed(format!(
                        "cdn.xporta.data_paths and poll_paths must not overlap (\"{}\")",
                        dp
                    )));
                }
            }
            let valid_enc = ["json", "binary"];
            if !valid_enc.contains(&xporta.encoding.as_str()) {
                return Err(ConfigError::ValidationFailed(format!(
                    "cdn.xporta.encoding must be one of: {:?}",
                    valid_enc
                )));
            }
        }
    }

    // Padding validation
    if config.padding.min > config.padding.max {
        return Err(ConfigError::ValidationFailed(
            "padding.min must be <= padding.max".into(),
        ));
    }

    // Camouflage validation
    if config.camouflage.tls_on_tcp && config.tls.is_none() {
        return Err(ConfigError::ValidationFailed(
            "camouflage.tls_on_tcp requires [tls] config (cert_path and key_path)".into(),
        ));
    }
    if config.camouflage.enabled && config.camouflage.fallback_addr.is_none() {
        tracing::warn!(
            "camouflage.enabled is true but fallback_addr is not set; \
             non-Prisma connections will be dropped instead of proxied to a decoy"
        );
    }

    // Port hopping validation
    if config.port_hopping.enabled {
        let end = config.port_hopping.base_port as u32 + config.port_hopping.port_range as u32;
        if end > 65535 {
            return Err(ConfigError::ValidationFailed(
                "port_hopping: base_port + port_range must not exceed 65535".into(),
            ));
        }
        if config.port_hopping.interval_secs == 0 {
            return Err(ConfigError::ValidationFailed(
                "port_hopping.interval_secs must be > 0".into(),
            ));
        }
        if config.port_hopping.grace_period_secs == 0 {
            return Err(ConfigError::ValidationFailed(
                "port_hopping.grace_period_secs must be > 0".into(),
            ));
        }
    }

    // Congestion control validation
    validate_congestion_config(&config.congestion)?;

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

    let valid_transports = ["quic", "tcp", "ws", "grpc", "xhttp", "xporta"];
    if !valid_transports.contains(&config.transport.as_str()) {
        return Err(ConfigError::ValidationFailed(format!(
            "transport must be one of: {:?}",
            valid_transports
        )));
    }

    // WS transport validation
    if config.transport == "ws" && config.ws_url.is_none() {
        return Err(ConfigError::ValidationFailed(
            "transport = \"ws\" requires ws_url".into(),
        ));
    }

    // gRPC transport validation
    if config.transport == "grpc" && config.grpc_url.is_none() {
        return Err(ConfigError::ValidationFailed(
            "transport = \"grpc\" requires grpc_url".into(),
        ));
    }

    // XHTTP transport validation
    if config.transport == "xhttp" {
        if config.xhttp_mode.is_none() {
            return Err(ConfigError::ValidationFailed(
                "transport = \"xhttp\" requires xhttp_mode".into(),
            ));
        }
        let mode = config.xhttp_mode.as_deref().unwrap();
        let valid_modes = ["packet-up", "stream-up", "stream-one"];
        if !valid_modes.contains(&mode) {
            return Err(ConfigError::ValidationFailed(format!(
                "xhttp_mode must be one of: {:?}",
                valid_modes
            )));
        }
        if mode == "stream-one" && config.xhttp_stream_url.is_none() {
            return Err(ConfigError::ValidationFailed(
                "xhttp_mode = \"stream-one\" requires xhttp_stream_url".into(),
            ));
        }
        if (mode == "packet-up" || mode == "stream-up")
            && (config.xhttp_upload_url.is_none() || config.xhttp_download_url.is_none())
        {
            return Err(ConfigError::ValidationFailed(
                "xhttp_mode \"packet-up\" or \"stream-up\" requires xhttp_upload_url and xhttp_download_url".into(),
            ));
        }
    }

    // XPorta transport validation
    if config.transport == "xporta" {
        let xporta = config.xporta.as_ref().ok_or_else(|| {
            ConfigError::ValidationFailed(
                "transport = \"xporta\" requires [xporta] config section".into(),
            )
        })?;
        if xporta.base_url.is_empty() {
            return Err(ConfigError::ValidationFailed(
                "xporta.base_url must not be empty".into(),
            ));
        }
        if !xporta.session_path.starts_with('/') {
            return Err(ConfigError::ValidationFailed(
                "xporta.session_path must start with '/'".into(),
            ));
        }
        if xporta.data_paths.is_empty() {
            return Err(ConfigError::ValidationFailed(
                "xporta.data_paths must not be empty".into(),
            ));
        }
        if xporta.poll_paths.is_empty() {
            return Err(ConfigError::ValidationFailed(
                "xporta.poll_paths must not be empty".into(),
            ));
        }
        for path in &xporta.data_paths {
            if !path.starts_with('/') {
                return Err(ConfigError::ValidationFailed(format!(
                    "xporta.data_paths entry \"{}\" must start with '/'",
                    path
                )));
            }
        }
        for path in &xporta.poll_paths {
            if !path.starts_with('/') {
                return Err(ConfigError::ValidationFailed(format!(
                    "xporta.poll_paths entry \"{}\" must start with '/'",
                    path
                )));
            }
        }
        // Check data_paths and poll_paths don't overlap
        for dp in &xporta.data_paths {
            if xporta.poll_paths.contains(dp) {
                return Err(ConfigError::ValidationFailed(format!(
                    "xporta.data_paths and poll_paths must not overlap (\"{}\" found in both)",
                    dp
                )));
            }
        }
        let valid_encodings = ["json", "binary", "auto"];
        if !valid_encodings.contains(&xporta.encoding.as_str()) {
            return Err(ConfigError::ValidationFailed(format!(
                "xporta.encoding must be one of: {:?}",
                valid_encodings
            )));
        }
        if !(1..=8).contains(&xporta.poll_concurrency) {
            return Err(ConfigError::ValidationFailed(
                "xporta.poll_concurrency must be 1-8".into(),
            ));
        }
        if !(1..=8).contains(&xporta.upload_concurrency) {
            return Err(ConfigError::ValidationFailed(
                "xporta.upload_concurrency must be 1-8".into(),
            ));
        }
        if !(10..=90).contains(&xporta.poll_timeout_secs) {
            return Err(ConfigError::ValidationFailed(
                "xporta.poll_timeout_secs must be 10-90".into(),
            ));
        }
    }

    // XMUX validation
    if let Some(ref xmux) = config.xmux {
        if xmux.max_connections_min > xmux.max_connections_max {
            return Err(ConfigError::ValidationFailed(
                "xmux.max_connections_min must be <= max_connections_max".into(),
            ));
        }
        if xmux.max_concurrency_min > xmux.max_concurrency_max {
            return Err(ConfigError::ValidationFailed(
                "xmux.max_concurrency_min must be <= max_concurrency_max".into(),
            ));
        }
        if xmux.max_lifetime_secs_min > xmux.max_lifetime_secs_max {
            return Err(ConfigError::ValidationFailed(
                "xmux.max_lifetime_secs_min must be <= max_lifetime_secs_max".into(),
            ));
        }
        if xmux.max_requests_min > xmux.max_requests_max {
            return Err(ConfigError::ValidationFailed(
                "xmux.max_requests_min must be <= max_requests_max".into(),
            ));
        }
    }

    validate_logging_level(&config.logging.level)?;
    validate_logging_format(&config.logging.format)?;

    // tls_on_tcp validation
    if config.tls_on_tcp {
        // Ensure we can derive a server name
        let has_server_name = config.tls_server_name.is_some()
            || config
                .server_addr
                .split(':')
                .next()
                .map(|h| !h.is_empty() && h.parse::<std::net::IpAddr>().is_err())
                .unwrap_or(false);
        if !has_server_name {
            return Err(ConfigError::ValidationFailed(
                "tls_on_tcp requires tls_server_name or a hostname (not IP) in server_addr".into(),
            ));
        }
    }

    // Congestion control validation
    validate_congestion_config(&config.congestion)?;

    // Port hopping validation
    if config.port_hopping.enabled {
        let end = config.port_hopping.base_port as u32 + config.port_hopping.port_range as u32;
        if end > 65535 {
            return Err(ConfigError::ValidationFailed(
                "port_hopping: base_port + port_range must not exceed 65535".into(),
            ));
        }
        if config.port_hopping.interval_secs == 0 {
            return Err(ConfigError::ValidationFailed(
                "port_hopping.interval_secs must be > 0".into(),
            ));
        }
        if config.port_hopping.grace_period_secs == 0 {
            return Err(ConfigError::ValidationFailed(
                "port_hopping.grace_period_secs must be > 0".into(),
            ));
        }
    }

    // Salamander password validation
    if let Some(ref password) = config.salamander_password {
        if password.is_empty() {
            return Err(ConfigError::ValidationFailed(
                "salamander_password must be non-empty when set".into(),
            ));
        }
    }

    // FEC validation
    if config.udp_fec.enabled {
        if config.udp_fec.data_shards == 0 {
            return Err(ConfigError::ValidationFailed(
                "udp_fec.data_shards must be > 0".into(),
            ));
        }
        if config.udp_fec.parity_shards == 0 {
            return Err(ConfigError::ValidationFailed(
                "udp_fec.parity_shards must be > 0".into(),
            ));
        }
    }

    // DNS mode validation
    {
        use crate::dns::DnsMode;
        match config.dns.mode {
            DnsMode::Smart | DnsMode::Fake | DnsMode::Tunnel | DnsMode::Direct => {}
        }
    }

    // DNS deep validation
    {
        use crate::dns::DnsMode;

        // Smart mode: warn if geosite_path is set but doesn't exist
        if config.dns.mode == DnsMode::Smart {
            if let Some(ref geosite_path) = config.dns.geosite_path {
                if !std::path::Path::new(geosite_path).exists() {
                    tracing::warn!(
                        "dns.geosite_path \"{}\" does not exist; \
                         smart DNS domain matching may not work correctly",
                        geosite_path
                    );
                }
            }
        }

        // fake_ip_range should be valid CIDR
        if !config.dns.fake_ip_range.contains('/') {
            return Err(ConfigError::ValidationFailed(
                "dns.fake_ip_range must be a valid CIDR (e.g., \"198.18.0.0/15\")".into(),
            ));
        }

        // upstream should contain ':' (IP:port format)
        if !config.dns.upstream.contains(':') {
            return Err(ConfigError::ValidationFailed(
                "dns.upstream must be in IP:port format (e.g., \"8.8.8.8:53\")".into(),
            ));
        }
    }

    // Routing rules validation
    for (i, rule) in config.routing.rules.iter().enumerate() {
        // Validate rule condition type (enforced by enum, but validate ip-cidr content)
        if let crate::router::RuleCondition::IpCidr(ref cidr) = rule.condition {
            if !cidr.contains('/') {
                return Err(ConfigError::ValidationFailed(format!(
                    "routing.rules[{}]: ip-cidr value \"{}\" must be valid CIDR format (must contain '/')",
                    i, cidr
                )));
            }
        }
    }

    // TUN validation
    if config.tun.enabled && !config.tun.dns.is_empty() {
        let valid_tun_dns = ["fake", "tunnel"];
        if !valid_tun_dns.contains(&config.tun.dns.as_str()) {
            return Err(ConfigError::ValidationFailed(format!(
                "tun.dns must be one of: {:?} when TUN is enabled",
                valid_tun_dns
            )));
        }
    }

    // TUN MTU validation
    if config.tun.enabled {
        if config.tun.mtu < 576 {
            return Err(ConfigError::ValidationFailed(format!(
                "tun.mtu must be >= 576 (got {})",
                config.tun.mtu
            )));
        }
        if config.tun.mtu > 9000 {
            return Err(ConfigError::ValidationFailed(format!(
                "tun.mtu must be <= 9000 (got {})",
                config.tun.mtu
            )));
        }

        // Validate include_routes CIDR format
        for (j, route) in config.tun.include_routes.iter().enumerate() {
            if !route.contains('/') {
                return Err(ConfigError::ValidationFailed(format!(
                    "tun.include_routes[{}] \"{}\" must be valid CIDR format (must contain '/')",
                    j, route
                )));
            }
        }

        // Validate exclude_routes CIDR format
        for (j, route) in config.tun.exclude_routes.iter().enumerate() {
            if !route.contains('/') {
                return Err(ConfigError::ValidationFailed(format!(
                    "tun.exclude_routes[{}] \"{}\" must be valid CIDR format (must contain '/')",
                    j, route
                )));
            }
        }
    }

    Ok(())
}

pub fn validate_logging_level(level: &str) -> Result<(), ConfigError> {
    let valid = ["trace", "debug", "info", "warn", "error"];
    if !valid.contains(&level) {
        return Err(ConfigError::ValidationFailed(format!(
            "logging.level must be one of: {:?}",
            valid
        )));
    }
    Ok(())
}

pub fn validate_logging_format(format: &str) -> Result<(), ConfigError> {
    let valid = ["pretty", "json"];
    if !valid.contains(&format) {
        return Err(ConfigError::ValidationFailed(format!(
            "logging.format must be one of: {:?}",
            valid
        )));
    }
    Ok(())
}

fn validate_congestion_config(config: &CongestionConfig) -> Result<(), ConfigError> {
    let valid_modes = ["brutal", "bbr", "adaptive"];
    if !valid_modes.contains(&config.mode.as_str()) {
        return Err(ConfigError::ValidationFailed(format!(
            "congestion.mode must be one of: {:?}",
            valid_modes
        )));
    }
    if (config.mode == "brutal" || config.mode == "adaptive") && config.target_bandwidth.is_none() {
        return Err(ConfigError::ValidationFailed(format!(
            "congestion.target_bandwidth must be set when mode is \"{}\"",
            config.mode
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{is_valid_bandwidth_format, is_valid_quota_format};
    use crate::util::hex_decode;

    #[test]
    fn test_hex_decode_valid() {
        assert_eq!(hex_decode("deadbeef"), Some(vec![0xde, 0xad, 0xbe, 0xef]));
    }

    #[test]
    fn test_hex_decode_invalid() {
        assert_eq!(hex_decode("xyz"), None);
    }

    #[test]
    fn test_bandwidth_format_valid() {
        assert!(is_valid_bandwidth_format("100mbps"));
        assert!(is_valid_bandwidth_format("1gbps"));
        assert!(is_valid_bandwidth_format("50kbps"));
        assert!(is_valid_bandwidth_format("1.5gbps"));
        assert!(is_valid_bandwidth_format("100Mbps"));
        assert!(is_valid_bandwidth_format("500KBPS"));
    }

    #[test]
    fn test_bandwidth_format_invalid() {
        assert!(!is_valid_bandwidth_format("100"));
        assert!(!is_valid_bandwidth_format("fast"));
        assert!(!is_valid_bandwidth_format("mbps"));
        assert!(!is_valid_bandwidth_format("100 mbps"));
        assert!(!is_valid_bandwidth_format("100tb"));
        assert!(!is_valid_bandwidth_format(""));
    }

    #[test]
    fn test_quota_format_valid() {
        assert!(is_valid_quota_format("100GB"));
        assert!(is_valid_quota_format("1TB"));
        assert!(is_valid_quota_format("500MB"));
        assert!(is_valid_quota_format("1024KB"));
        assert!(is_valid_quota_format("1.5tb"));
        assert!(is_valid_quota_format("100gb"));
    }

    #[test]
    fn test_quota_format_invalid() {
        assert!(!is_valid_quota_format("100"));
        assert!(!is_valid_quota_format("large"));
        assert!(!is_valid_quota_format("GB"));
        assert!(!is_valid_quota_format("100 GB"));
        assert!(!is_valid_quota_format("100PB"));
        assert!(!is_valid_quota_format(""));
    }
}
