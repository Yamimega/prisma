use anyhow::Result;

pub fn run_validate(config_path: &str, config_type: &str) -> Result<()> {
    match config_type {
        "server" => validate_server(config_path),
        "client" => validate_client(config_path),
        _ => Err(anyhow::anyhow!(
            "Unknown config type '{}'. Use 'server' or 'client'.",
            config_type
        )),
    }
}

fn validate_server(path: &str) -> Result<()> {
    println!("Validating server config: {}", path);

    let config =
        prisma_core::config::load_server_config(path).map_err(|e| anyhow::anyhow!("{}", e))?;

    println!("  OK - Config is valid");
    println!();
    println!("  Listen (TCP):    {}", config.listen_addr);
    println!("  Listen (QUIC):   {}", config.quic_listen_addr);
    println!(
        "  TLS:             {}",
        if config.tls.is_some() {
            "configured"
        } else {
            "not configured"
        }
    );
    println!("  Clients:         {}", config.authorized_clients.len());
    println!(
        "  Camouflage:      {}",
        if config.camouflage.enabled {
            "enabled"
        } else {
            "disabled"
        }
    );
    println!(
        "  CDN:             {}",
        if config.cdn.enabled {
            format!("enabled ({})", config.cdn.listen_addr)
        } else {
            "disabled".to_string()
        }
    );
    println!(
        "  Management API:  {}",
        if config.management_api.enabled {
            format!("enabled ({})", config.management_api.listen_addr)
        } else {
            "disabled".to_string()
        }
    );
    println!(
        "  Port Forwarding: {}",
        if config.port_forwarding.enabled {
            "enabled"
        } else {
            "disabled"
        }
    );

    if config.cdn.enabled {
        println!();
        println!("  CDN Details:");
        println!("    WS tunnel path:   {}", config.cdn.ws_tunnel_path);
        println!("    gRPC tunnel path: {}", config.cdn.grpc_tunnel_path);
        if let Some(ref upstream) = config.cdn.cover_upstream {
            println!("    Cover upstream:   {}", upstream);
        }
        if let Some(ref dir) = config.cdn.cover_static_dir {
            println!("    Cover static dir: {}", dir);
        }
        println!("    Expose mgmt API: {}", config.cdn.expose_management_api);
    }

    Ok(())
}

fn validate_client(path: &str) -> Result<()> {
    println!("Validating client config: {}", path);

    let config =
        prisma_core::config::load_client_config(path).map_err(|e| anyhow::anyhow!("{}", e))?;

    println!("  OK - Config is valid");
    println!();
    println!("  SOCKS5:    {}", config.socks5_listen_addr);
    if let Some(ref http) = config.http_listen_addr {
        println!("  HTTP:      {}", http);
    }
    println!("  Server:    {}", config.server_addr);
    println!("  Transport: {}", config.transport);
    println!("  Cipher:    {}", config.cipher_suite);

    if config.transport == "ws" {
        if let Some(ref url) = config.ws_url {
            println!("  WS URL:    {}", url);
        }
    }
    if config.transport == "grpc" {
        if let Some(ref url) = config.grpc_url {
            println!("  gRPC URL:  {}", url);
        }
    }

    if !config.port_forwards.is_empty() {
        println!("  Port Forwards: {}", config.port_forwards.len());
    }

    Ok(())
}
