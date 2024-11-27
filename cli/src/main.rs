use anyhow::{bail, ensure};
use clap::{Parser, Subcommand};
use ethers::types::{Address, H256};
use intmax2_cli::cli::{
    deposit::deposit_ft,
    get::balance,
    send::tx,
    sync::{sync, sync_withdrawals},
};
use intmax2_client_sdk::utils::init_logger::init_logger;
use intmax2_interfaces::data::deposit_data::TokenType;
use intmax2_zkp::{
    common::{generic_address::GenericAddress, signature::key_set::KeySet},
    ethereum_types::{address::Address as IAddress, u256::U256, u32limb_trait::U32LimbTrait as _},
};
use num_bigint::BigUint;

#[derive(Parser)]
#[clap(name = "intmax2_cli")]
#[clap(about = "Intmax2 CLI tool")]
struct Args {
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Tx {
        #[clap(long)]
        private_key: H256,
        #[clap(long)]
        to: String,
        #[clap(long)]
        amount: u128,
        #[clap(long)]
        token_index: u32,
    },
    Deposit {
        #[clap(long)]
        private_key: H256,
        #[clap(long)]
        amount: u128,
        #[clap(long)]
        token_type: TokenType,
        #[clap(long)]
        token_address: Address,
        #[clap(long)]
        token_id: Option<u128>,
    },
    Sync {
        #[clap(long)]
        private_key: H256,
    },
    SyncWithdrawals {
        #[clap(long)]
        private_key: H256,
    },
    Balance {
        #[clap(long)]
        private_key: H256,
    },

    GenerateKey,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_logger();
    let args = Args::parse();

    dotenv::dotenv().ok();

    match args.command {
        Commands::Tx {
            private_key,
            to,
            amount,
            token_index,
        } => {
            let to = parse_generic_address(&to)?;
            let key = h256_to_keyset(private_key);
            tx(key, to, amount.into(), token_index).await?;
        }
        Commands::Deposit {
            private_key,
            amount,
            token_type,
            token_address,
            token_id,
        } => {
            let token_id = token_id.map(|id| id.into());
            let key = h256_to_keyset(private_key);
            deposit_ft(
                key,
                private_key,
                amount.into(),
                token_type,
                token_address,
                token_id,
            )
            .await?;
        }
        Commands::Sync { private_key } => {
            let key = h256_to_keyset(private_key);
            sync(key).await?;
        }
        Commands::SyncWithdrawals { private_key } => {
            let key = h256_to_keyset(private_key);
            sync_withdrawals(key).await?;
        }
        Commands::Balance { private_key } => {
            let key = h256_to_keyset(private_key);
            balance(key).await?;
        }
        Commands::GenerateKey => {
            println!("Generating key");
            let mut rng = rand::thread_rng();
            let key = KeySet::rand(&mut rng);
            let private_key = BigUint::from(key.privkey);
            let private_key: U256 = private_key.try_into().unwrap();
            println!("Private key: {}", private_key.to_hex());
            println!("Public key: {}", key.pubkey.to_hex());
        }
    }
    Ok(())
}

fn parse_generic_address(address: &str) -> anyhow::Result<GenericAddress> {
    ensure!(address.starts_with("0x"), "Invalid prefix");
    let bytes = hex::decode(&address[2..])?;
    if bytes.len() == 20 {
        let address = IAddress::from_bytes_be(&bytes);
        return Ok(GenericAddress::from_address(address));
    } else if bytes.len() == 32 {
        let pubkey = U256::from_bytes_be(&bytes);
        return Ok(GenericAddress::from_pubkey(pubkey));
    } else {
        bail!("Invalid length");
    }
}

fn h256_to_keyset(h256: H256) -> KeySet {
    KeySet::new(BigUint::from_bytes_be(h256.as_bytes()).into())
}
