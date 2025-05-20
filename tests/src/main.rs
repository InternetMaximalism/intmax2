use alloy::primitives::B256;
use clap::{Parser, Subcommand};
use intmax2_cli::cli::client::get_client;
use tests::config::TestConfig;

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
    TransferLoop {
        #[clap(long)]
        eth_private_key: B256,
    },
    MiningLoop {
        #[clap(long)]
        eth_private_key: B256,
    },
    Info {
        #[clap(long)]
        eth_private_key: B256,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    let args = Args::parse();
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .init();

    let client = get_client()?;
    let config = TestConfig::load_from_env()?;

    match args.command {
        Commands::Info { eth_private_key } => {
            tests::utils::print_info(&client, eth_private_key).await?;
        }
        Commands::BridgeLoop {
            eth_private_key,
            from_withdrawal,
        } => {
            tests::bridge_loop::bridge_loop(&config, &client, eth_private_key, from_withdrawal)
                .await?;
        }
        Commands::TransferLoop { eth_private_key } => {
            tests::transfer_loop::transfer_loop(&config, &client, eth_private_key).await?;
        }
        Commands::MiningLoop { eth_private_key } => {
            tests::mining_loop::mining_loop(&config, &client, eth_private_key).await?;
        }
    }
    Ok(())
}
