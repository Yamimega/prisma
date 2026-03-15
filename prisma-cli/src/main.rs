mod api_client;
mod bandwidth;
mod clients;
mod completions;
mod config_ops;
mod connections;
mod dashboard;
mod diagnostics;
mod init;
mod logs;
mod metrics;
mod routes;
mod status;
mod validate;

use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const PROTOCOL_VERSION: u8 = prisma_core::types::PRISMA_PROTOCOL_VERSION;

#[derive(Parser)]
#[command(name = "prisma", about = "Prisma proxy infrastructure suite")]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Output raw JSON instead of formatted tables
    #[arg(long, global = true)]
    json: bool,

    /// Management API URL (overrides env PRISMA_MGMT_URL and auto-detect)
    #[arg(long, global = true, env = "PRISMA_MGMT_URL")]
    mgmt_url: Option<String>,

    /// Management API auth token (overrides env PRISMA_MGMT_TOKEN and auto-detect)
    #[arg(long, global = true, env = "PRISMA_MGMT_TOKEN")]
    mgmt_token: Option<String>,
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
        /// Management API URL (hidden, use --mgmt-url instead)
        #[arg(short, long, hide = true)]
        url: Option<String>,
        /// Auth token (hidden, use --mgmt-token instead)
        #[arg(short, long, hide = true)]
        token: Option<String>,
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
    /// Launch the web dashboard (auto-downloads UI, proxies management API)
    Dashboard {
        /// Management API URL to proxy requests to
        #[arg(long, default_value = "https://127.0.0.1:9090")]
        mgmt_url: String,
        /// Auth token for management API
        #[arg(long)]
        token: Option<String>,
        /// Port to serve the dashboard on
        #[arg(long, default_value = "9091")]
        port: u16,
        /// Address to bind the dashboard server to
        #[arg(long, default_value = "0.0.0.0")]
        bind: String,
        /// Don't auto-open browser
        #[arg(long)]
        no_open: bool,
        /// Force re-download of dashboard assets
        #[arg(long)]
        update: bool,
        /// Serve dashboard from a local directory instead of downloading
        #[arg(long)]
        dir: Option<String>,
    },
    /// Generate shell completion scripts
    Completions {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },

    // --- Management API commands ---
    /// Manage authorized clients
    #[command(subcommand)]
    Clients(ClientsCmd),
    /// Manage active connections
    #[command(subcommand)]
    Connections(ConnectionsCmd),
    /// View server metrics and system info
    Metrics {
        /// Auto-refresh metrics
        #[arg(long)]
        watch: bool,
        /// Show historical metrics
        #[arg(long)]
        history: bool,
        /// History period: 1h, 6h, 24h, 7d
        #[arg(long, default_value = "1h")]
        period: String,
        /// Refresh interval in seconds (for --watch)
        #[arg(long, default_value = "2")]
        interval: u64,
        /// Show system info instead of metrics
        #[arg(long)]
        system: bool,
    },
    /// Manage bandwidth limits and quotas
    #[command(subcommand)]
    Bandwidth(BandwidthCmd),
    /// Manage server configuration
    #[command(subcommand)]
    Config(ConfigCmd),
    /// Manage routing rules
    #[command(subcommand)]
    Routes(RoutesCmd),
    /// Stream live server logs via WebSocket
    Logs {
        /// Minimum log level: TRACE, DEBUG, INFO, WARN, ERROR
        #[arg(long)]
        level: Option<String>,
        /// Maximum number of log lines to display
        #[arg(long)]
        lines: Option<usize>,
    },
    /// Ping the server (connect + handshake RTT)
    Ping {
        /// Path to client config file
        #[arg(short, long, default_value = "client.toml")]
        config: String,
        /// Override server address
        #[arg(short, long)]
        server: Option<String>,
        /// Number of pings
        #[arg(long, default_value = "5")]
        count: u32,
        /// Interval between pings in milliseconds
        #[arg(long, default_value = "1000")]
        interval: u64,
    },
    /// Test all configured transports against the server
    TestTransport {
        /// Path to client config file
        #[arg(short, long, default_value = "client.toml")]
        config: String,
    },
}

#[derive(Subcommand)]
enum ClientsCmd {
    /// List all authorized clients
    List,
    /// Show details for a specific client
    Show {
        /// Client ID
        id: String,
    },
    /// Create a new client
    Create {
        /// Client name
        #[arg(long)]
        name: Option<String>,
    },
    /// Delete a client
    Delete {
        /// Client ID
        id: String,
        /// Skip confirmation
        #[arg(long)]
        yes: bool,
    },
    /// Enable a client
    Enable {
        /// Client ID
        id: String,
    },
    /// Disable a client
    Disable {
        /// Client ID
        id: String,
    },
}

#[derive(Subcommand)]
enum ConnectionsCmd {
    /// List active connections
    List,
    /// Disconnect a specific connection
    Disconnect {
        /// Session ID
        id: String,
    },
    /// Watch connections in real-time
    Watch {
        /// Refresh interval in seconds
        #[arg(long, default_value = "2")]
        interval: u64,
    },
}

#[derive(Subcommand)]
enum BandwidthCmd {
    /// Show bandwidth summary for all clients
    Summary,
    /// Show bandwidth and quota for a specific client
    Get {
        /// Client ID
        id: String,
    },
    /// Set bandwidth limits for a client
    Set {
        /// Client ID
        id: String,
        /// Upload limit in bits per second (0 = unlimited)
        #[arg(long)]
        upload: Option<u64>,
        /// Download limit in bits per second (0 = unlimited)
        #[arg(long)]
        download: Option<u64>,
    },
    /// Get or set traffic quota for a client
    Quota {
        /// Client ID
        id: String,
        /// Quota limit in bytes (omit to show current)
        #[arg(long)]
        limit: Option<u64>,
    },
}

#[derive(Subcommand)]
enum ConfigCmd {
    /// Show current server configuration
    Get,
    /// Update a configuration value
    Set {
        /// Configuration key (dotted notation, e.g., logging.level)
        key: String,
        /// New value
        value: String,
    },
    /// Show TLS configuration
    Tls,
    /// Manage configuration backups
    #[command(subcommand)]
    Backup(BackupCmd),
}

#[derive(Subcommand)]
enum BackupCmd {
    /// Create a new backup
    Create,
    /// List all backups
    List,
    /// Restore a backup
    Restore {
        /// Backup name
        name: String,
    },
    /// Show diff between backup and current config
    Diff {
        /// Backup name
        name: String,
    },
    /// Delete a backup
    Delete {
        /// Backup name
        name: String,
    },
}

#[derive(Subcommand)]
enum RoutesCmd {
    /// List all routing rules
    List,
    /// Create a new routing rule
    Create {
        /// Rule name
        #[arg(long)]
        name: String,
        /// Condition (TYPE:VALUE, e.g., DomainMatch:*.ads.*, IpCidr:10.0.0.0/8, PortRange:80-443, All)
        #[arg(long)]
        condition: String,
        /// Action: allow or block
        #[arg(long)]
        action: String,
        /// Priority (lower = higher priority)
        #[arg(long, default_value = "100")]
        priority: u32,
    },
    /// Update a routing rule
    Update {
        /// Rule ID
        id: String,
        /// New condition
        #[arg(long)]
        condition: Option<String>,
        /// New action
        #[arg(long)]
        action: Option<String>,
        /// New priority
        #[arg(long)]
        priority: Option<u32>,
        /// New name
        #[arg(long)]
        name: Option<String>,
    },
    /// Delete a routing rule
    Delete {
        /// Rule ID
        id: String,
    },
    /// Apply a predefined routing rule preset
    Setup {
        /// Preset name: block-ads, privacy, allow-all, block-all
        preset: String,
        /// Delete all existing rules before applying preset
        #[arg(long)]
        clear: bool,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install default CryptoProvider");

    let cli = Cli::parse();
    let global_json = cli.json;
    let global_mgmt_url = cli.mgmt_url;
    let global_mgmt_token = cli.mgmt_token;

    match cli.command {
        Commands::Server { config } => {
            let path = resolve_config(&config, "server.toml");
            prisma_server::run(path.to_str().unwrap()).await?;
        }
        Commands::Client { config } => {
            let path = resolve_config(&config, "client.toml");
            prisma_client::run(path.to_str().unwrap()).await?;
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
            let client = api_client::ApiClient::resolve(
                url.as_deref().or(global_mgmt_url.as_deref()),
                token.as_deref().or(global_mgmt_token.as_deref()),
                global_json,
            )?;
            status::run_status(&client)?;
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
        Commands::Dashboard {
            mgmt_url,
            token,
            port,
            bind,
            no_open,
            update,
            dir,
        } => {
            dashboard::run_dashboard(mgmt_url, token, port, bind, no_open, update, dir).await?;
        }
        Commands::Completions { shell } => {
            completions::generate(shell);
        }

        // --- Management API commands ---
        Commands::Clients(cmd) => {
            let client = api_client::ApiClient::resolve(
                global_mgmt_url.as_deref(),
                global_mgmt_token.as_deref(),
                global_json,
            )?;
            match cmd {
                ClientsCmd::List => clients::list(&client)?,
                ClientsCmd::Show { id } => clients::show(&client, &id)?,
                ClientsCmd::Create { name } => clients::create(&client, name.as_deref())?,
                ClientsCmd::Delete { id, .. } => clients::delete(&client, &id)?,
                ClientsCmd::Enable { id } => clients::enable(&client, &id)?,
                ClientsCmd::Disable { id } => clients::disable(&client, &id)?,
            }
        }
        Commands::Connections(cmd) => {
            let client = api_client::ApiClient::resolve(
                global_mgmt_url.as_deref(),
                global_mgmt_token.as_deref(),
                global_json,
            )?;
            match cmd {
                ConnectionsCmd::List => connections::list(&client)?,
                ConnectionsCmd::Disconnect { id } => connections::disconnect(&client, &id)?,
                ConnectionsCmd::Watch { interval } => connections::watch(&client, interval)?,
            }
        }
        Commands::Metrics {
            watch,
            history,
            system,
            period,
            interval,
        } => {
            let client = api_client::ApiClient::resolve(
                global_mgmt_url.as_deref(),
                global_mgmt_token.as_deref(),
                global_json,
            )?;
            if system {
                metrics::system(&client)?;
            } else if history {
                metrics::history(&client, &period)?;
            } else if watch {
                metrics::watch(&client, interval)?;
            } else {
                metrics::snapshot(&client)?;
            }
        }
        Commands::Bandwidth(cmd) => {
            let client = api_client::ApiClient::resolve(
                global_mgmt_url.as_deref(),
                global_mgmt_token.as_deref(),
                global_json,
            )?;
            match cmd {
                BandwidthCmd::Summary => bandwidth::summary(&client)?,
                BandwidthCmd::Get { id } => bandwidth::get(&client, &id)?,
                BandwidthCmd::Set {
                    id,
                    upload,
                    download,
                } => bandwidth::set(&client, &id, upload, download)?,
                BandwidthCmd::Quota { id, limit } => bandwidth::quota(&client, &id, limit)?,
            }
        }
        Commands::Config(cmd) => {
            let client = api_client::ApiClient::resolve(
                global_mgmt_url.as_deref(),
                global_mgmt_token.as_deref(),
                global_json,
            )?;
            match cmd {
                ConfigCmd::Get => config_ops::get_config(&client)?,
                ConfigCmd::Set { key, value } => config_ops::set_config(&client, &key, &value)?,
                ConfigCmd::Tls => config_ops::tls(&client)?,
                ConfigCmd::Backup(bcmd) => match bcmd {
                    BackupCmd::Create => config_ops::backup_create(&client)?,
                    BackupCmd::List => config_ops::backup_list(&client)?,
                    BackupCmd::Restore { name } => config_ops::backup_restore(&client, &name)?,
                    BackupCmd::Diff { name } => config_ops::backup_diff(&client, &name)?,
                    BackupCmd::Delete { name } => config_ops::backup_delete(&client, &name)?,
                },
            }
        }
        Commands::Routes(cmd) => {
            let client = api_client::ApiClient::resolve(
                global_mgmt_url.as_deref(),
                global_mgmt_token.as_deref(),
                global_json,
            )?;
            match cmd {
                RoutesCmd::List => routes::list(&client)?,
                RoutesCmd::Create {
                    name,
                    condition,
                    action,
                    priority,
                } => routes::create(&client, &name, &condition, &action, priority)?,
                RoutesCmd::Update {
                    id,
                    condition,
                    action,
                    priority,
                    name,
                } => routes::update(
                    &client,
                    &id,
                    condition.as_deref(),
                    action.as_deref(),
                    priority,
                    name.as_deref(),
                )?,
                RoutesCmd::Delete { id } => routes::delete(&client, &id)?,
                RoutesCmd::Setup { preset, clear } => routes::setup(&client, &preset, clear)?,
            }
        }
        Commands::Logs { level, lines } => {
            let client = api_client::ApiClient::resolve(
                global_mgmt_url.as_deref(),
                global_mgmt_token.as_deref(),
                global_json,
            )?;
            logs::stream(&client, level.as_deref(), lines).await?;
        }
        Commands::Ping {
            config,
            server,
            count,
            interval,
        } => {
            let path = resolve_config(&config, "client.toml");
            diagnostics::ping(path.to_str().unwrap(), server.as_deref(), count, interval).await?;
        }
        Commands::TestTransport { config } => {
            let path = resolve_config(&config, "client.toml");
            diagnostics::test_transport(path.to_str().unwrap()).await?;
        }
    }

    Ok(())
}

/// Resolve config file path. If the given path exists, use it directly.
/// Otherwise search standard locations: /etc/prisma/, ~/.config/prisma/.
fn resolve_config(given: &str, default_name: &str) -> PathBuf {
    let given_path = Path::new(given);
    if given_path.exists() {
        return given_path.to_path_buf();
    }

    // Only search fallback locations when the user didn't provide an explicit path
    // (i.e. they're using the clap default value).
    if given == default_name {
        let candidates: Vec<PathBuf> = if cfg!(windows) {
            // %PROGRAMDATA%\prisma\ and %USERPROFILE%\.config\prisma\
            let mut v = Vec::new();
            if let Ok(pd) = std::env::var("PROGRAMDATA") {
                v.push(PathBuf::from(pd).join("prisma").join(default_name));
            }
            if let Ok(home) = std::env::var("USERPROFILE") {
                v.push(
                    PathBuf::from(home)
                        .join(".config")
                        .join("prisma")
                        .join(default_name),
                );
            }
            v
        } else {
            let mut v = vec![PathBuf::from("/etc/prisma").join(default_name)];
            if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
                v.push(PathBuf::from(xdg).join("prisma").join(default_name));
            } else if let Ok(home) = std::env::var("HOME") {
                v.push(
                    PathBuf::from(home)
                        .join(".config")
                        .join("prisma")
                        .join(default_name),
                );
            }
            v
        };

        for candidate in &candidates {
            if candidate.exists() {
                eprintln!("Using config: {}", candidate.display());
                return candidate.clone();
            }
        }

        // Nothing found — print helpful message and fall through to the default
        eprintln!(
            "Config file '{}' not found in current directory or standard locations:",
            default_name
        );
        eprintln!("  - ./{}", default_name);
        for c in &candidates {
            eprintln!("  - {}", c.display());
        }
        eprintln!();
        eprintln!("Run 'prisma init' to generate config files, or pass --config <path>.");
    }

    given_path.to_path_buf()
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
        // v4 fields
        protocol_version: config.protocol_version.clone(),
        fingerprint: config.fingerprint.clone(),
        quic_version: config.quic_version.clone(),
        traffic_shaping: config.traffic_shaping.clone(),
        use_prisma_tls: config.transport == "prisma-tls" || config.transport == "reality",
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

    println!("  Tunnel established (v4, 1 RTT)");
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
    println!();
    println!("Protocol features:");
    println!("  - 2-step handshake (1 RTT)");
    println!("  - 0-RTT session resumption with tickets");
    println!("  - PrismaUDP relay (CMD_UDP_ASSOCIATE/UDP_DATA)");
    println!("  - Encrypted DNS queries (CMD_DNS_QUERY/DNS_RESPONSE)");
    println!("  - Speed test (CMD_SPEED_TEST)");
    println!("  - Challenge-response verification");
    println!("  - 2-byte flags with FEC, priority, compression support");
    println!("  - Server feature negotiation bitmask");
    println!("  - Brutal/BBR/Adaptive congestion control");
    println!("  - Port hopping (QUIC, HMAC-based)");
    println!("  - Salamander v2 UDP obfuscation (nonce-based)");
    println!("  - Forward Error Correction (Reed-Solomon)");
    println!("  - Smart/Fake/Tunnel DNS modes");
    println!("  - Rule-based routing (domain/IP/port)");
    println!("  - Per-client bandwidth limits and traffic quotas");
    println!("  - XPorta transport (REST API simulation, CDN-compatible)");
    println!("  - Bucket padding (anti-encapsulated-TLS fingerprinting)");
    println!("  - Traffic shaping (chaff, jitter, coalescing)");
    println!("  - PrismaTLS (replaces REALITY, padding beacon auth)");
    println!("  - PrismaFP (browser fingerprint mimicry)");
    println!("  - Entropy camouflage (GFW exemption)");
    println!("  - TUN mode");
    println!();
    println!("Supported ciphers:");
    println!("  - chacha20-poly1305 (default)");
    println!("  - aes-256-gcm");
    println!();
    println!("Supported transports:");
    println!("  - quic      (QUIC v2, default)");
    println!("  - prisma-tls (TCP + PrismaTLS)");
    println!("  - ws        (WebSocket, CDN-compatible)");
    println!("  - grpc      (gRPC, CDN-compatible)");
    println!("  - xhttp     (HTTP-native, CDN-compatible)");
    println!("  - xporta    (REST API simulation, CDN-compatible)");
}
