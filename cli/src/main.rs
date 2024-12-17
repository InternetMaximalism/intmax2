use clap::Parser;
use colored::Colorize as _;
use intmax2_cli::{
    args::{Args, Commands},
    cli::{
        claim::claim_withdrawals,
        deposit::deposit,
        error::CliError,
        get::{balance, history, withdrawal_status},
        send::{transfer, TransferInput},
        sync::sync_withdrawals,
        utils::post_empty_block,
    },
    format::{format_token_info, h256_to_keyset},
};
use intmax2_client_sdk::utils::logger::init_logger;
use intmax2_zkp::{
    common::signature::key_set::KeySet,
    ethereum_types::{u256::U256 as IU256, u32limb_trait::U32LimbTrait},
};
use num_bigint::BigUint;

const MAX_BATCH_TRANSFER: usize = 5;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_logger();
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
        } => {
            let key = h256_to_keyset(private_key);
            let transfer_input = TransferInput {
                recipient: to,
                amount: amount.into(),
                token_index,
            };
            transfer(key, &[transfer_input]).await?;
        }
        Commands::BatchTransfer {
            private_key,
            csv_path,
        } => {
            let key = h256_to_keyset(private_key);
            let mut reader = csv::Reader::from_path(csv_path)?;
            let mut transfers = vec![];
            for result in reader.deserialize() {
                let transfer_input: TransferInput = result?;
                transfers.push(transfer_input);
            }
            if transfers.len() > MAX_BATCH_TRANSFER {
                return Err(CliError::TooManyTransfer(transfers.len()));
            }
            transfer(key, &transfers).await?;
        }
        Commands::Deposit {
            eth_private_key,
            private_key,
            amount,
            token_type,
            token_address,
            token_id,
        } => {
            let key = h256_to_keyset(private_key);
            let amount = amount.map(|x| x.into());
            let token_id = token_id.map(|x| x.into());
            let (amount, token_address, token_id) =
                format_token_info(token_type, amount, token_address, token_id)?;
            deposit(
                key,
                eth_private_key,
                token_type,
                amount,
                token_address,
                token_id,
            )
            .await?;
        }
        Commands::SyncWithdrawals { private_key } => {
            let key = h256_to_keyset(private_key);
            sync_withdrawals(key).await?;
        }
        Commands::PostEmptyBlock => {
            post_empty_block().await?;
        }
        Commands::Balance { private_key } => {
            let key = h256_to_keyset(private_key);
            balance(key).await?;
        }
        Commands::History { private_key } => {
            let key = h256_to_keyset(private_key);
            history(key).await?;
        }
        Commands::WithdrawalStatus { private_key } => {
            let key = h256_to_keyset(private_key);
            withdrawal_status(key).await?;
        }
        Commands::ClaimWithdrawals {
            private_key,
            eth_private_key,
        } => {
            let key = h256_to_keyset(private_key);
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
