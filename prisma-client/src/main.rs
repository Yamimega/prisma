use clap::Parser;

#[derive(Parser)]
#[command(name = "prisma-client", about = "Prisma proxy client")]
struct Args {
    /// Path to client config file
    #[arg(short, long, default_value = "client.toml")]
    config: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    prisma_client::run(&args.config).await
}
