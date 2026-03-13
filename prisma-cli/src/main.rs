mod init;
mod status;
mod validate;

use clap::{Parser, Subcommand};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const PROTOCOL_VERSION: u8 = prisma_core::types::PROTOCOL_VERSION;

#[derive(Parser)]
#[command(name = "prisma", about = "Prisma proxy infrastructure suite")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the proxy server
    Server {
        /// Path to server config file
        #[arg(short, long, default_value = "server.toml")]
        config: String,
    },
    /// Start the proxy client
    Client {
        /// Path to client config file
        #[arg(short, long, default_value = "client.toml")]
        config: String,
    },
    /// Generate a new client key (UUID + auth secret)
    GenKey,
    /// Generate a self-signed TLS certificate for development
    GenCert {
        /// Output directory for cert and key files
        #[arg(short, long, default_value = ".")]
        output: String,
        /// Common name for the certificate
        #[arg(long, default_value = "prisma-server")]
        cn: String,
    },
    /// Generate annotated config files with auto-generated keys
    Init {
        /// Include CDN section pre-configured
        #[arg(long)]
        cdn: bool,
        /// Generate only server config
        #[arg(long)]
        server_only: bool,
        /// Generate only client config
        #[arg(long)]
        client_only: bool,
        /// Overwrite existing files
        #[arg(long)]
        force: bool,
    },
    /// Validate a config file without starting
    Validate {
        /// Path to config file
        #[arg(short, long)]
        config: String,
        /// Config type: 'server' or 'client'
        #[arg(short = 't', long, default_value = "server")]
        r#type: String,
    },
    /// Query management API for server status
    Status {
        /// Management API URL
        #[arg(short, long, default_value = "http://127.0.0.1:9090")]
        url: String,
        /// Auth token for management API
        #[arg(short, long, default_value = "")]
        token: String,
    },
    /// Run a speed test against the server
    SpeedTest {
        /// Server address (host:port)
        #[arg(short, long)]
        server: String,
        /// Duration in seconds
        #[arg(short, long, default_value = "10")]
        duration: u64,
        /// Direction: "download", "upload", or "both"
        #[arg(long, default_value = "both")]
        direction: String,
        /// Path to client config file (for auth credentials)
        #[arg(short = 'C', long, default_value = "client.toml")]
        config: String,
    },
    /// Show version, protocol version, supported ciphers and transports
    Version,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install default CryptoProvider");

    let cli = Cli::parse();

    match cli.command {
        Commands::Server { config } => {
            prisma_server::run(&config).await?;
        }
        Commands::Client { config } => {
            prisma_client::run(&config).await?;
        }
        Commands::GenKey => {
            gen_key();
        }
        Commands::GenCert { output, cn } => {
            gen_cert(&output, &cn)?;
        }
        Commands::Init {
            cdn,
            server_only,
            client_only,
            force,
        } => {
            init::run_init(cdn, server_only, client_only, force)?;
        }
        Commands::Validate { config, r#type } => {
            validate::run_validate(&config, &r#type)?;
        }
        Commands::Status { url, token } => {
            status::run_status(&url, &token)?;
        }
        Commands::SpeedTest {
            server,
            duration,
            direction,
            config,
        } => {
            run_speed_test(&server, duration, &direction, &config).await?;
        }
        Commands::Version => {
            print_version();
        }
    }

    Ok(())
}

fn gen_key() {
    let client_id = uuid::Uuid::new_v4();
    let mut secret = [0u8; 32];
    rand::Rng::fill(&mut rand::thread_rng(), &mut secret);
    let secret_hex: String = secret.iter().map(|b| format!("{:02x}", b)).collect();

    println!("Client ID:   {}", client_id);
    println!("Auth Secret: {}", secret_hex);
    println!();
    println!("# Add to server.toml:");
    println!("[[authorized_clients]]");
    println!("id = \"{}\"", client_id);
    println!("auth_secret = \"{}\"", secret_hex);
    println!("name = \"my-client\"");
    println!();
    println!("# Add to client.toml:");
    println!("[identity]");
    println!("client_id = \"{}\"", client_id);
    println!("auth_secret = \"{}\"", secret_hex);
}

fn gen_cert(output: &str, cn: &str) -> anyhow::Result<()> {
    let mut params = rcgen::CertificateParams::new(vec![cn.to_string()])?;
    params
        .subject_alt_names
        .push(rcgen::SanType::DnsName(cn.to_string().try_into()?));

    let key_pair = rcgen::KeyPair::generate()?;
    let cert = params.self_signed(&key_pair)?;

    let cert_path = format!("{}/prisma-cert.pem", output);
    let key_path = format!("{}/prisma-key.pem", output);

    std::fs::write(&cert_path, cert.pem())?;
    std::fs::write(&key_path, key_pair.serialize_pem())?;

    println!("Certificate: {}", cert_path);
    println!("Private key: {}", key_path);

    Ok(())
}

async fn run_speed_test(
    server: &str,
    duration: u64,
    direction: &str,
    config_path: &str,
) -> anyhow::Result<()> {
    use prisma_core::config::load_client_config;
    use prisma_core::congestion::CongestionMode;
    use prisma_core::crypto::aead::AeadCipher;
    use prisma_core::protocol::codec::*;
    use prisma_core::protocol::types::*;
    use prisma_core::router::Router;
    use prisma_core::types::{CipherSuite, ClientId, MAX_FRAME_SIZE};
    use std::sync::Arc;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let config = load_client_config(config_path)
        .map_err(|e| anyhow::anyhow!("Failed to load client config: {}", e))?;

    println!("Prisma Speed Test");
    println!("  Server:    {}", server);
    println!("  Duration:  {}s", duration);
    println!("  Direction: {}", direction);
    println!("  Client ID: {}", config.identity.client_id);
    println!();

    let server_addr = if server.contains(':') {
        server.to_string()
    } else {
        format!("{}:8443", server)
    };

    let client_id = ClientId::from_uuid(
        uuid::Uuid::parse_str(&config.identity.client_id)
            .map_err(|e| anyhow::anyhow!("Invalid client_id: {}", e))?,
    );
    let auth_secret = prisma_core::util::hex_decode_32(&config.identity.auth_secret)
        .map_err(|e| anyhow::anyhow!("Invalid auth_secret: {}", e))?;
    let cipher_suite = match config.cipher_suite.as_str() {
        "aes-256-gcm" => CipherSuite::Aes256Gcm,
        _ => CipherSuite::ChaCha20Poly1305,
    };

    let congestion_mode = CongestionMode::from_config(
        &config.congestion.mode,
        config.congestion.target_bandwidth.as_deref(),
    );

    // Connect to server
    println!("Connecting to {}...", server_addr);
    let connect_start = std::time::Instant::now();

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
        router: Arc::new(Router::new(vec![])),
    };

    let transport = ctx.connect().await?;
    let rtt = connect_start.elapsed();
    println!("  Connected in {:.1}ms", rtt.as_secs_f64() * 1000.0);

    // Establish raw tunnel (handshake only, no CONNECT command)
    let tunnel = prisma_client::tunnel::establish_raw_tunnel(
        transport,
        client_id,
        auth_secret,
        cipher_suite,
    )
    .await?;

    println!("  Tunnel established (v3, 1 RTT)");
    println!();

    let (mut tunnel_read, mut tunnel_write) = tokio::io::split(tunnel.stream);
    let cipher: Arc<dyn AeadCipher> = Arc::from(tunnel.cipher);
    let mut session_keys = tunnel.session_keys;

    // Run download test
    if direction == "download" || direction == "both" {
        println!("Download test ({} seconds)...", duration);

        // Send CMD_SPEED_TEST requesting download
        let frame = DataFrame {
            command: Command::SpeedTest {
                direction: 0, // download
                duration_secs: duration as u8,
                data: vec![],
            },
            flags: 0,
            stream_id: 0,
        };
        let frame_bytes = encode_data_frame(&frame);
        let nonce = session_keys.next_client_nonce();
        let encrypted = encrypt_frame(cipher.as_ref(), &nonce, &frame_bytes)?;
        let len = (encrypted.len() as u16).to_be_bytes();
        tunnel_write.write_all(&len).await?;
        tunnel_write.write_all(&encrypted).await?;

        // Receive data for duration_secs and measure throughput
        let test_start = std::time::Instant::now();
        let mut total_bytes: u64 = 0;
        let mut frame_buf = vec![0u8; MAX_FRAME_SIZE];

        while test_start.elapsed().as_secs() < duration + 2 {
            let mut len_buf = [0u8; 2];
            match tokio::time::timeout(
                std::time::Duration::from_secs(3),
                tunnel_read.read_exact(&mut len_buf),
            )
            .await
            {
                Ok(Ok(_)) => {}
                _ => break,
            }
            let frame_len = u16::from_be_bytes(len_buf) as usize;
            if frame_len > MAX_FRAME_SIZE {
                break;
            }
            frame_buf.resize(frame_len, 0);
            if tunnel_read
                .read_exact(&mut frame_buf[..frame_len])
                .await
                .is_err()
            {
                break;
            }
            total_bytes += frame_len as u64 + 2;

            // Decrypt to check if it's a SpeedTest frame or Close
            if let Ok((plaintext, _)) = decrypt_frame(cipher.as_ref(), &frame_buf[..frame_len]) {
                if let Ok(f) = decode_data_frame(&plaintext) {
                    match f.command {
                        Command::SpeedTest { .. } => {
                            // Count the data payload
                        }
                        Command::Close => break,
                        _ => {}
                    }
                }
            }
        }

        let elapsed = test_start.elapsed().as_secs_f64();
        let mbps = (total_bytes as f64 * 8.0) / elapsed / 1_000_000.0;
        println!(
            "  Downloaded: {:.2} MB in {:.1}s",
            total_bytes as f64 / 1_048_576.0,
            elapsed
        );
        println!("  Speed: {:.2} Mbps", mbps);
        println!();
    }

    // Run upload test
    if direction == "upload" || direction == "both" {
        println!("Upload test ({} seconds)...", duration);

        // Send CMD_SPEED_TEST requesting upload
        let frame = DataFrame {
            command: Command::SpeedTest {
                direction: 1, // upload
                duration_secs: duration as u8,
                data: vec![],
            },
            flags: 0,
            stream_id: 0,
        };
        let frame_bytes = encode_data_frame(&frame);
        let nonce = session_keys.next_client_nonce();
        let encrypted = encrypt_frame(cipher.as_ref(), &nonce, &frame_bytes)?;
        let len = (encrypted.len() as u16).to_be_bytes();
        tunnel_write.write_all(&len).await?;
        tunnel_write.write_all(&encrypted).await?;

        // Send random data for duration_secs
        let test_start = std::time::Instant::now();
        let mut total_bytes: u64 = 0;
        let random_payload = vec![0xABu8; 8192];

        while test_start.elapsed().as_secs() < duration {
            let frame = DataFrame {
                command: Command::SpeedTest {
                    direction: 1,
                    duration_secs: 0,
                    data: random_payload.clone(),
                },
                flags: 0,
                stream_id: 0,
            };
            let frame_bytes = encode_data_frame(&frame);
            let nonce = session_keys.next_client_nonce();
            let encrypted = encrypt_frame(cipher.as_ref(), &nonce, &frame_bytes)?;
            let len = (encrypted.len() as u16).to_be_bytes();
            if tunnel_write.write_all(&len).await.is_err() {
                break;
            }
            if tunnel_write.write_all(&encrypted).await.is_err() {
                break;
            }
            total_bytes += encrypted.len() as u64 + 2;
        }

        let elapsed = test_start.elapsed().as_secs_f64();
        let mbps = (total_bytes as f64 * 8.0) / elapsed / 1_000_000.0;
        println!(
            "  Uploaded: {:.2} MB in {:.1}s",
            total_bytes as f64 / 1_048_576.0,
            elapsed
        );
        println!("  Speed: {:.2} Mbps", mbps);
        println!();
    }

    println!("Speed test complete.");
    Ok(())
}

fn print_version() {
    println!(
        "prisma {} (PrismaVeil Protocol v{})",
        VERSION, PROTOCOL_VERSION
    );
    println!("Backward-compatible with v1 and v2 clients");
    println!();
    println!("v3 features:");
    println!("  - 2-step handshake (1 RTT, down from 2 RTT)");
    println!("  - 0-RTT session resumption with tickets");
    println!("  - PrismaUDP relay (CMD_UDP_ASSOCIATE/UDP_DATA)");
    println!("  - Encrypted DNS queries (CMD_DNS_QUERY/DNS_RESPONSE)");
    println!("  - Speed test (CMD_SPEED_TEST)");
    println!("  - Challenge-response verification");
    println!("  - 2-byte flags with FEC, priority, compression support");
    println!("  - Server feature negotiation bitmask");
    println!("  - Brutal/BBR/Adaptive congestion control");
    println!("  - Port hopping (QUIC, HMAC-based)");
    println!("  - Salamander UDP obfuscation");
    println!("  - Forward Error Correction (Reed-Solomon)");
    println!("  - Smart/Fake/Tunnel DNS modes");
    println!("  - Rule-based routing (domain/IP/port)");
    println!("  - Per-client bandwidth limits and traffic quotas");
    println!("  - XPorta transport (REST API simulation, CDN-compatible)");
    println!();
    println!("v2 features:");
    println!("  - Per-frame padding with configurable ranges");
    println!("  - XHTTP transport modes (packet-up, stream-up, stream-one)");
    println!("  - XMUX connection multiplexing");
    println!("  - HTTP header obfuscation");
    println!();
    println!("Supported ciphers:");
    println!("  - chacha20-poly1305 (default)");
    println!("  - aes-256-gcm");
    println!();
    println!("Supported transports:");
    println!("  - quic  (default)");
    println!("  - tcp");
    println!("  - ws    (WebSocket, CDN-compatible)");
    println!("  - grpc  (gRPC, CDN-compatible)");
    println!("  - xhttp  (HTTP-native, CDN-compatible)");
    println!("  - xporta (REST API simulation, CDN-compatible)");
}
