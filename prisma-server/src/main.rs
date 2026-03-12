use clap::Parser;

#[derive(Parser)]
#[command(name = "prisma-server", about = "Prisma proxy server")]
struct Args {
    /// Path to server config file
    #[arg(short, long, default_value = "server.toml")]
    config: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    prisma_server::run(&args.config).await
}
