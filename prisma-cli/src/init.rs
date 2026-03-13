use std::path::Path;

use anyhow::Result;

pub fn run_init(cdn: bool, server_only: bool, client_only: bool, force: bool) -> Result<()> {
    let client_id = uuid::Uuid::new_v4();
    let mut secret = [0u8; 32];
    rand::Rng::fill(&mut rand::thread_rng(), &mut secret);
    let secret_hex: String = secret.iter().map(|b| format!("{:02x}", b)).collect();

    let mut mgmt_token_bytes = [0u8; 24];
    rand::Rng::fill(&mut rand::thread_rng(), &mut mgmt_token_bytes);
    let mgmt_token: String = mgmt_token_bytes
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect();

    if !client_only {
        write_server_config(&client_id.to_string(), &secret_hex, &mgmt_token, cdn, force)?;
    }

    if !server_only {
        write_client_config(&client_id.to_string(), &secret_hex, cdn, force)?;
    }

    println!();
    println!("Generated credentials:");
    println!("  Client ID:   {}", client_id);
    println!("  Auth Secret: {}", secret_hex);
    println!("  Mgmt Token:  {}", mgmt_token);

    // Generate TLS cert if not present
    if !client_only && !Path::new("prisma-cert.pem").exists() {
        println!();
        println!("Generating self-signed TLS certificate...");
        let key_pair = rcgen::KeyPair::generate()?;
        let mut params = rcgen::CertificateParams::new(vec!["prisma-server".to_string()])?;
        params.subject_alt_names.push(rcgen::SanType::DnsName(
            "prisma-server".to_string().try_into()?,
        ));
        let cert = params.self_signed(&key_pair)?;
        std::fs::write("prisma-cert.pem", cert.pem())?;
        std::fs::write("prisma-key.pem", key_pair.serialize_pem())?;
        println!("  Certificate: prisma-cert.pem");
        println!("  Private key: prisma-key.pem");
    }

    Ok(())
}

fn write_server_config(
    client_id: &str,
    secret_hex: &str,
    mgmt_token: &str,
    cdn: bool,
    force: bool,
) -> Result<()> {
    let path = "server.toml";
    if Path::new(path).exists() && !force {
        println!(
            "Skipping {} (already exists, use --force to overwrite)",
            path
        );
        return Ok(());
    }

    let cdn_section = if cdn {
        r#"
# CDN-compatible transport (WebSocket + gRPC through Cloudflare)
[cdn]
enabled = true
listen_addr = "0.0.0.0:443"
ws_tunnel_path = "/ws-tunnel"
grpc_tunnel_path = "/tunnel.PrismaTunnel"
# cover_upstream = "http://127.0.0.1:3000"     # Reverse proxy to a real website as cover
# cover_static_dir = "/var/www/html"            # OR serve static files as cover
# trusted_proxies = ["173.245.48.0/20"]         # Cloudflare IP ranges
expose_management_api = true
management_api_path = "/prisma-mgmt"

[cdn.tls]
cert_path = "origin-cert.pem"                   # Cloudflare Origin Certificate
key_path = "origin-key.pem"
"#
        .to_string()
    } else {
        r#"
# CDN-compatible transport (WebSocket + gRPC through Cloudflare)
# [cdn]
# enabled = true
# listen_addr = "0.0.0.0:443"
# ws_tunnel_path = "/ws-tunnel"
# grpc_tunnel_path = "/tunnel.PrismaTunnel"
# cover_upstream = "http://127.0.0.1:3000"
# expose_management_api = true
# management_api_path = "/prisma-mgmt"
#
# [cdn.tls]
# cert_path = "origin-cert.pem"
# key_path = "origin-key.pem"
"#
        .to_string()
    };

    let content = format!(
        r#"listen_addr = "0.0.0.0:8443"
quic_listen_addr = "0.0.0.0:8443"

[tls]
cert_path = "prisma-cert.pem"
key_path = "prisma-key.pem"

# Generate keys with: prisma gen-key
[[authorized_clients]]
id = "{client_id}"
auth_secret = "{secret_hex}"
name = "my-client"

[logging]
level = "info"
format = "pretty"

[performance]
max_connections = 1024
connection_timeout_secs = 300

# Port forwarding (reverse proxy) — allow clients to expose local services
[port_forwarding]
enabled = true
port_range_start = 10000
port_range_end = 20000

# Camouflage — anti-active-detection for censorship resistance
# [camouflage]
# enabled = true
# tls_on_tcp = true
# fallback_addr = "example.com:443"
# alpn_protocols = ["h2", "http/1.1"]
{cdn_section}
# Management API + dashboard
[management_api]
enabled = true
listen_addr = "127.0.0.1:9090"
auth_token = "{mgmt_token}"
# dashboard_dir = "./prisma-dashboard/out"
# cors_origins = ["http://localhost:3000"]
"#
    );

    std::fs::write(path, content)?;
    println!("Created {}", path);
    Ok(())
}

fn write_client_config(client_id: &str, secret_hex: &str, cdn: bool, force: bool) -> Result<()> {
    let path = "client.toml";
    if Path::new(path).exists() && !force {
        println!(
            "Skipping {} (already exists, use --force to overwrite)",
            path
        );
        return Ok(());
    }

    let transport_section = if cdn {
        r#"transport = "ws"
ws_url = "wss://your-domain.com/ws-tunnel"
skip_cert_verify = false

# Alternative: gRPC transport
# transport = "grpc"
# grpc_url = "https://your-domain.com/tunnel.PrismaTunnel/Tunnel""#
            .to_string()
    } else {
        r#"transport = "quic"
skip_cert_verify = true"#
            .to_string()
    };

    let server_addr = if cdn {
        "your-domain.com:443"
    } else {
        "127.0.0.1:8443"
    };

    let content = format!(
        r#"socks5_listen_addr = "127.0.0.1:1080"
http_listen_addr = "127.0.0.1:8080"
server_addr = "{server_addr}"
cipher_suite = "chacha20-poly1305"
{transport_section}

[identity]
client_id = "{client_id}"
auth_secret = "{secret_hex}"

# Port forwarding — expose local services through the server
# [[port_forwards]]
# name = "my-web-app"
# local_addr = "127.0.0.1:3000"
# remote_port = 10080

# WebSocket transport options (when transport = "ws")
# ws_url = "wss://domain.com/ws-tunnel"
# ws_host = "domain.com"                        # Override Host header
# ws_extra_headers = [["X-Custom", "value"]]    # Custom headers

# gRPC transport options (when transport = "grpc")
# grpc_url = "https://domain.com/tunnel.PrismaTunnel/Tunnel"

# Camouflage — must match server camouflage settings
# tls_on_tcp = true
# tls_server_name = "example.com"
# alpn_protocols = ["h2", "http/1.1"]

[logging]
level = "info"
format = "pretty"
"#
    );

    std::fs::write(path, content)?;
    println!("Created {}", path);
    Ok(())
}
