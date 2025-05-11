use alloy::primitives::B256;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[clap(name = "intmax2_test_cli")]
#[clap(about = "Test CLI tool for Intmax2")]
pub struct Args {
    #[clap(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    BridgeLoop {
        #[clap(long)]
        eth_private_key: B256,
        #[clap(long, default_value_t = false)]
        from_withdrawal: bool,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv::dotenv().ok();
    let args = Args::parse();
    match args.command {
        Commands::BridgeLoop {
            eth_private_key,
            from_withdrawal,
        } => {
            tests::bridge_loop::bridge_loop(eth_private_key, from_withdrawal).await?;
        }
    }
    Ok(())
}
