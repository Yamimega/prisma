use clap::{Parser, Subcommand};

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
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
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
