use clap::Parser;
use colored::Colorize as _;
use ethers::types::H256;
use intmax2_cli::{
    args::{Args, Commands},
    cli::{
        claim::claim_withdrawals,
        deposit::deposit,
        error::CliError,
        get::{balance, claim_status, history, mining_list, withdrawal_status},
        send::{transfer, TransferInput},
        sync::{sync_claims, sync_withdrawals},
    },
    format::{format_token_info, privkey_to_keyset},
};
use intmax2_zkp::{
    common::signature::key_set::KeySet,
    ethereum_types::{u256::U256 as IU256, u32limb_trait::U32LimbTrait},
};
use num_bigint::BigUint;

const MAX_BATCH_TRANSFER: usize = 5;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .init();
    let args = Args::parse();

    dotenv::dotenv().ok();

    match main_process(args.command).await {
        Ok(_) => {}
        Err(e) => {
            if matches!(e, CliError::PendingTxError) {
                println!(
                    "{}",
                    "There are pending sent tx. Please try again later.".red()
                );
                std::process::exit(1);
            }
            println!("{}", e.to_string().red());
            std::process::exit(1);
        }
    }
    Ok(())
}

async fn main_process(command: Commands) -> Result<(), CliError> {
    match command {
        Commands::Transfer {
            private_key,
            to,
            amount,
            token_index,
            fee_token_index,
        } => {
            let key = privkey_to_keyset(private_key);
            let transfer_input = TransferInput {
                recipient: to,
                amount,
                token_index,
            };
            transfer(key, &[transfer_input], fee_token_index.unwrap_or_default()).await?;
        }
        Commands::BatchTransfer {
            private_key,
            csv_path,
            fee_token_index,
        } => {
            let key = privkey_to_keyset(private_key);
            let mut reader = csv::Reader::from_path(csv_path)?;
            let mut transfers = vec![];
            for result in reader.deserialize() {
                let transfer_input: TransferInput = result?;
                transfers.push(transfer_input);
            }
            if transfers.len() > MAX_BATCH_TRANSFER {
                return Err(CliError::TooManyTransfer(transfers.len()));
            }
            transfer(key, &transfers, fee_token_index.unwrap_or_default()).await?;
        }
        Commands::Deposit {
            eth_private_key,
            private_key,
            amount,
            token_type,
            token_address,
            token_id,
            is_mining,
        } => {
            let key = privkey_to_keyset(private_key);
            let amount = amount.map(|x| x.into());
            let token_id = token_id.map(|x| x.into());
            let (amount, token_address, token_id) =
                format_token_info(token_type, amount, token_address, token_id)?;
            let is_mining = is_mining.unwrap_or(false);
            deposit(
                key,
                eth_private_key,
                token_type,
                amount,
                token_address,
                token_id,
                is_mining,
            )
            .await?;
        }
        Commands::SyncWithdrawals { private_key } => {
            let key = privkey_to_keyset(private_key);
            sync_withdrawals(key).await?;
        }
        Commands::SyncClaims {
            private_key,
            recipient,
        } => {
            let key = privkey_to_keyset(private_key);
            sync_claims(key, recipient).await?;
        }
        Commands::Balance { private_key } => {
            let key = generate_key(private_key);
            balance(key).await?;
        }
        Commands::History { private_key } => {
            let key = generate_key(private_key);
            history(key).await?;
        }
        Commands::WithdrawalStatus { private_key } => {
            let key = privkey_to_keyset(private_key);
            withdrawal_status(key).await?;
        }
        Commands::MiningList { private_key } => {
            let key = privkey_to_keyset(private_key);
            mining_list(key).await?;
        }
        Commands::ClaimStatus { private_key } => {
            let key = privkey_to_keyset(private_key);
            claim_status(key).await?;
        }
        Commands::ClaimWithdrawals {
            private_key,
            eth_private_key,
        } => {
            let key = privkey_to_keyset(private_key);
            claim_withdrawals(key, eth_private_key).await?;
        }
        Commands::GenerateKey => {
            let mut rng = rand::thread_rng();
            let key = KeySet::rand(&mut rng);
            let private_key = BigUint::from(key.privkey);
            let private_key: IU256 = private_key.try_into().unwrap();
            println!("Private key: {}", private_key.to_hex());
            println!("Public key: {}", key.pubkey.to_hex());
        }
        Commands::GenerateFromEthKey { eth_private_key } => {
            let provisional = BigUint::from_bytes_be(eth_private_key.as_bytes());
            let key = KeySet::generate_from_provisional(provisional.into());
            let private_key = BigUint::from(key.privkey);
            let private_key: IU256 = private_key.try_into().unwrap();
            println!("Private key: {}", private_key.to_hex());
            println!("Public key: {}", key.pubkey.to_hex());
        }
    }
    Ok(())
}

fn generate_key(private_key: Option<H256>) -> KeySet {
    match private_key {
        Some(private_key) => privkey_to_keyset(private_key),
        None => {
            let pubkey: H256 = std::env::var("PUBKEY").unwrap().parse().unwrap();
            let mut rng = rand::thread_rng();
            let mut key = KeySet::rand(&mut rng);
            key.pubkey = BigUint::from_bytes_be(pubkey.as_bytes())
                .try_into()
                .unwrap();
            key
        }
    }
}
