use std::sync::Arc;

use anyhow::Result;

pub async fn ping(
    config_path: &str,
    server: Option<&str>,
    count: u32,
    interval_ms: u64,
) -> Result<()> {
    use prisma_core::config::load_client_config;
    use prisma_core::congestion::CongestionMode;

    let config = load_client_config(config_path)
        .map_err(|e| anyhow::anyhow!("Failed to load client config: {}", e))?;

    let server_addr = server
        .map(|s| s.to_string())
        .unwrap_or_else(|| config.server_addr.clone());
    let server_addr = if server_addr.contains(':') {
        server_addr
    } else {
        format!("{}:8443", server_addr)
    };

    println!("PRISMA PING {} ({})", server_addr, config_path);
    println!();

    let client_id = prisma_core::types::ClientId::from_uuid(
        uuid::Uuid::parse_str(&config.identity.client_id)
            .map_err(|e| anyhow::anyhow!("Invalid client_id: {}", e))?,
    );
    let auth_secret = prisma_core::util::hex_decode_32(&config.identity.auth_secret)
        .map_err(|e| anyhow::anyhow!("Invalid auth_secret: {}", e))?;
    let cipher_suite = match config.cipher_suite.as_str() {
        "aes-256-gcm" => prisma_core::types::CipherSuite::Aes256Gcm,
        _ => prisma_core::types::CipherSuite::ChaCha20Poly1305,
    };
    let congestion_mode = CongestionMode::from_config(
        &config.congestion.mode,
        config.congestion.target_bandwidth.as_deref(),
    );

    let ctx = prisma_client::proxy::ProxyContext {
        server_addr: server_addr.clone(),
        client_id,
        auth_secret,
        cipher_suite,
        use_quic: config.transport == "quic",
        skip_cert_verify: config.skip_cert_verify,
        tls_on_tcp: config.tls_on_tcp,
        alpn_protocols: config.alpn_protocols.clone(),
        tls_server_name: config.tls_server_name.clone(),
        use_ws: config.transport == "ws",
        ws_url: config.ws_url.clone(),
        ws_extra_headers: config.ws_extra_headers.clone(),
        use_grpc: config.transport == "grpc",
        grpc_url: config.grpc_url.clone(),
        use_xhttp: config.transport == "xhttp",
        xhttp_mode: config.xhttp_mode.clone(),
        xhttp_stream_url: config.xhttp_stream_url.clone(),
        xhttp_upload_url: config.xhttp_upload_url.clone(),
        xhttp_download_url: config.xhttp_download_url.clone(),
        xhttp_extra_headers: config.xhttp_extra_headers.clone(),
        use_xporta: config.transport == "xporta",
        xporta_config: config.xporta.clone(),
        user_agent: config.user_agent.clone(),
        referer: config.referer.clone(),
        congestion_mode,
        port_hopping: config.port_hopping.clone(),
        salamander_password: config.salamander_password.clone(),
        udp_fec: None,
        dns_config: prisma_core::dns::DnsConfig::default(),
        dns_resolver: prisma_client::dns_resolver::DnsResolver::new(
            &prisma_core::dns::DnsConfig::default(),
        ),
        router: Arc::new(prisma_core::router::Router::new(vec![])),
        protocol_version: config.protocol_version.clone(),
        fingerprint: config.fingerprint.clone(),
        quic_version: config.quic_version.clone(),
        traffic_shaping: config.traffic_shaping.clone(),
        use_prisma_tls: config.transport == "prisma-tls" || config.transport == "reality",
    };

    let mut rtts = Vec::new();

    for seq in 1..=count {
        let start = std::time::Instant::now();
        match ctx.connect().await {
            Ok(transport) => {
                // Establish tunnel handshake to measure full RTT
                match prisma_client::tunnel::establish_raw_tunnel(
                    transport,
                    client_id,
                    auth_secret,
                    cipher_suite,
                )
                .await
                {
                    Ok(_tunnel) => {
                        let rtt = start.elapsed();
                        let ms = rtt.as_secs_f64() * 1000.0;
                        rtts.push(ms);
                        println!(
                            "seq={} transport={} time={:.1}ms",
                            seq, config.transport, ms
                        );
                    }
                    Err(e) => {
                        println!("seq={} tunnel error: {}", seq, e);
                    }
                }
            }
            Err(e) => {
                println!("seq={} connect error: {}", seq, e);
            }
        }

        if seq < count {
            tokio::time::sleep(std::time::Duration::from_millis(interval_ms)).await;
        }
    }

    println!();
    if rtts.is_empty() {
        println!("--- no successful pings ---");
    } else {
        let min = rtts.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = rtts.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let avg = rtts.iter().sum::<f64>() / rtts.len() as f64;
        println!("--- {} ping statistics ---", server_addr);
        println!(
            "{} transmitted, {} received, {:.0}% loss",
            count,
            rtts.len(),
            (1.0 - rtts.len() as f64 / count as f64) * 100.0
        );
        println!("rtt min/avg/max = {:.1}/{:.1}/{:.1} ms", min, avg, max);
    }

    Ok(())
}

pub async fn test_transport(config_path: &str) -> Result<()> {
    use prisma_core::config::load_client_config;
    use prisma_core::congestion::CongestionMode;

    let config = load_client_config(config_path)
        .map_err(|e| anyhow::anyhow!("Failed to load client config: {}", e))?;

    let client_id = prisma_core::types::ClientId::from_uuid(
        uuid::Uuid::parse_str(&config.identity.client_id)
            .map_err(|e| anyhow::anyhow!("Invalid client_id: {}", e))?,
    );
    let auth_secret = prisma_core::util::hex_decode_32(&config.identity.auth_secret)
        .map_err(|e| anyhow::anyhow!("Invalid auth_secret: {}", e))?;
    let cipher_suite = match config.cipher_suite.as_str() {
        "aes-256-gcm" => prisma_core::types::CipherSuite::Aes256Gcm,
        _ => prisma_core::types::CipherSuite::ChaCha20Poly1305,
    };
    let congestion_mode = CongestionMode::from_config(
        &config.congestion.mode,
        config.congestion.target_bandwidth.as_deref(),
    );

    let server_addr = if config.server_addr.contains(':') {
        config.server_addr.clone()
    } else {
        format!("{}:8443", config.server_addr)
    };

    println!("Testing transports to {} ...", server_addr);
    println!();

    let transports = [
        ("quic", true),
        (
            "prisma-tls",
            true, // Always available as TCP transport
        ),
        ("ws", config.ws_url.is_some()),
        ("grpc", config.grpc_url.is_some()),
        (
            "xhttp",
            config.xhttp_stream_url.is_some() || config.xhttp_upload_url.is_some(),
        ),
        ("xporta", config.xporta.is_some()),
    ];

    let mut rows = Vec::new();

    for (transport_name, available) in &transports {
        if !available {
            rows.push(vec![
                transport_name.to_string(),
                "SKIP".to_string(),
                "-".to_string(),
                "not configured".to_string(),
            ]);
            continue;
        }

        let ctx = prisma_client::proxy::ProxyContext {
            server_addr: server_addr.clone(),
            client_id,
            auth_secret,
            cipher_suite,
            use_quic: *transport_name == "quic",
            skip_cert_verify: config.skip_cert_verify,
            tls_on_tcp: config.tls_on_tcp,
            alpn_protocols: config.alpn_protocols.clone(),
            tls_server_name: config.tls_server_name.clone(),
            use_ws: *transport_name == "ws",
            ws_url: config.ws_url.clone(),
            ws_extra_headers: config.ws_extra_headers.clone(),
            use_grpc: *transport_name == "grpc",
            grpc_url: config.grpc_url.clone(),
            use_xhttp: *transport_name == "xhttp",
            xhttp_mode: config.xhttp_mode.clone(),
            xhttp_stream_url: config.xhttp_stream_url.clone(),
            xhttp_upload_url: config.xhttp_upload_url.clone(),
            xhttp_download_url: config.xhttp_download_url.clone(),
            xhttp_extra_headers: config.xhttp_extra_headers.clone(),
            use_xporta: *transport_name == "xporta",
            xporta_config: config.xporta.clone(),
            user_agent: config.user_agent.clone(),
            referer: config.referer.clone(),
            congestion_mode: congestion_mode.clone(),
            port_hopping: config.port_hopping.clone(),
            salamander_password: config.salamander_password.clone(),
            udp_fec: None,
            dns_config: prisma_core::dns::DnsConfig::default(),
            dns_resolver: prisma_client::dns_resolver::DnsResolver::new(
                &prisma_core::dns::DnsConfig::default(),
            ),
            router: Arc::new(prisma_core::router::Router::new(vec![])),
            protocol_version: config.protocol_version.clone(),
            fingerprint: config.fingerprint.clone(),
            quic_version: config.quic_version.clone(),
            traffic_shaping: config.traffic_shaping.clone(),
            use_prisma_tls: *transport_name == "prisma-tls" || *transport_name == "reality",
        };

        let start = std::time::Instant::now();
        match tokio::time::timeout(std::time::Duration::from_secs(10), ctx.connect()).await {
            Ok(Ok(_transport)) => {
                let latency = start.elapsed();
                rows.push(vec![
                    transport_name.to_string(),
                    "OK".to_string(),
                    format!("{:.0}ms", latency.as_secs_f64() * 1000.0),
                    String::new(),
                ]);
            }
            Ok(Err(e)) => {
                rows.push(vec![
                    transport_name.to_string(),
                    "FAIL".to_string(),
                    "-".to_string(),
                    format!("{}", e),
                ]);
            }
            Err(_) => {
                rows.push(vec![
                    transport_name.to_string(),
                    "TIMEOUT".to_string(),
                    ">10s".to_string(),
                    "connection timed out".to_string(),
                ]);
            }
        }
    }

    crate::api_client::print_table(&["Transport", "Status", "Latency", "Error"], &rows);
    Ok(())
}
