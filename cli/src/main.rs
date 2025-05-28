use clap::Parser;
#[cfg(not(windows))]
use colored::Colorize as _;
#[cfg(windows)]
use colored::{control, Colorize as _};
use intmax2_cli::{
    args::{Args, Commands},
    cli::{
        backup::{incorporate_backup, make_history_backup},
        claim::{claim_builder_reward, claim_withdrawals},
        deposit::deposit,
        error::CliError,
        get::{
            balance, check_validity_prover, claim_status, get_payment_memos, get_user_data,
            mining_list, withdrawal_status,
        },
        history::history,
        key_derivation::derive_key_from_eth,
        send::send_transfers,
        sync::{resync, sync_claims, sync_withdrawals},
        withdrawal::send_withdrawal,
    },
    format::{format_token_info, parse_generic_address, privkey_to_keyset},
};
use intmax2_client_sdk::client::{
    key_from_eth::generate_intmax_account_from_eth_key, sync::utils::generate_salt,
};
use intmax2_interfaces::utils::random::default_rng;
use intmax2_zkp::{
    common::{signature_content::key_set::KeySet, transfer::Transfer},
    ethereum_types::{u256::U256, u32limb_trait::U32LimbTrait},
};
use serde::Deserialize;

const MAX_BATCH_TRANSFER: usize = 63;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        control::set_virtual_terminal(true).unwrap();
    }

    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .init();
    let args = Args::parse();

    dotenvy::dotenv().ok();

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
            wait,
        } => {
            let key = privkey_to_keyset(private_key);
            let transfer = Transfer {
                recipient: U256::from(to).into(),
                amount,
                token_index,
                salt: generate_salt(),
            };
            send_transfers(
                key,
                &[transfer],
                vec![],
                fee_token_index.unwrap_or_default(),
                wait,
            )
            .await?;
        }
        Commands::Withdrawal {
            private_key,
            to,
            amount,
            token_index,
            fee_token_index,
            with_claim_fee,
            wait,
        } => {
            let key = privkey_to_keyset(private_key);
            let fee_token_index = fee_token_index.unwrap_or(0);
            send_withdrawal(
                key,
                to,
                amount,
                token_index,
                fee_token_index,
                with_claim_fee,
                wait,
            )
            .await?;
        }
        Commands::BatchTransfer {
            private_key,
            csv_path,
            fee_token_index,
            wait,
        } => {
            let key = privkey_to_keyset(private_key);
            let mut reader = csv::Reader::from_path(csv_path)?;
            let mut transfers = vec![];
            for result in reader.deserialize() {
                let transfer_input: TransferInput = result?;
                transfers.push(Transfer {
                    recipient: parse_generic_address(&transfer_input.recipient)
                        .map_err(|e| CliError::ParseError(e.to_string()))?,
                    amount: transfer_input.amount,
                    token_index: transfer_input.token_index,
                    salt: generate_salt(),
                });
            }
            if transfers.len() > MAX_BATCH_TRANSFER {
                return Err(CliError::TooManyTransfer(transfers.len()));
            }
            send_transfers(
                key,
                &transfers,
                vec![],
                fee_token_index.unwrap_or_default(),
                wait,
            )
            .await?;
        }
        Commands::Deposit {
            eth_private_key,
            private_key,
            amount,
            token_type,
            token_address,
            token_id,
            mining,
        } => {
            let key = privkey_to_keyset(private_key);
            let (amount, token_address, token_id) =
                format_token_info(token_type, amount, token_address, token_id)?;
            deposit(
                key,
                eth_private_key,
                token_type,
                amount,
                token_address,
                token_id,
                mining,
            )
            .await?;
        }
        Commands::SyncWithdrawals {
            private_key,
            fee_token_index,
        } => {
            let key = privkey_to_keyset(private_key);
            sync_withdrawals(key, fee_token_index).await?;
        }
        Commands::SyncClaims {
            private_key,
            recipient,
            fee_token_index,
        } => {
            let key = privkey_to_keyset(private_key);
            sync_claims(key, recipient, fee_token_index).await?;
        }
        Commands::ClaimBuilderReward { eth_private_key } => {
            claim_builder_reward(eth_private_key).await?;
        }
        Commands::Balance {
            private_key,
            without_sync,
        } => {
            let key = privkey_to_keyset(private_key);
            balance(key, !without_sync).await?;
        }
        Commands::UserData { private_key } => {
            let key = privkey_to_keyset(private_key);
            get_user_data(key).await?;
        }
        Commands::History {
            private_key,
            order,
            from,
        } => {
            let key = privkey_to_keyset(private_key);
            let order = order.unwrap_or_default();
            history(key, order, from).await?;
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
        Commands::PaymentMemos { private_key, name } => {
            let key = privkey_to_keyset(private_key);
            get_payment_memos(key, &name).await?;
        }
        Commands::ClaimWithdrawals {
            private_key,
            eth_private_key,
        } => {
            let key = privkey_to_keyset(private_key);
            claim_withdrawals(key, eth_private_key).await?;
        }
        Commands::Resync { private_key, deep } => {
            let key = privkey_to_keyset(private_key);
            resync(key, deep).await?;
        }
        Commands::MakeBackup {
            private_key,
            dir,
            from,
        } => {
            let key = privkey_to_keyset(private_key);
            let from = from.unwrap_or_default();
            let dir = dir.unwrap_or_default();
            make_history_backup(key, &dir, from).await?;
        }
        Commands::IncorporateBackup { path } => {
            incorporate_backup(&path)?;
        }
        Commands::CheckValidityProver => {
            check_validity_prover().await?;
        }
        Commands::GenerateKey => {
            let mut rng = default_rng();
            let key = KeySet::rand(&mut rng);
            println!("Private key: {}", key.privkey.to_hex());
            println!("Public key: {}", key.pubkey.to_hex());
        }
        Commands::PublicKey { private_key } => {
            let key = KeySet::new(private_key.into());
            println!("Public key: {}", key.pubkey.to_hex());
        }
        Commands::KeyFromBackupKey { backup_key } => {
            let key = generate_intmax_account_from_eth_key(backup_key);
            println!("Private key: {}", key.privkey.to_hex());
            println!("Public key: {}", key.pubkey.to_hex());
        }
        Commands::KeyFromEth {
            eth_private_key,
            redeposit_index,
            wallet_index,
        } => {
            let key = derive_key_from_eth(
                eth_private_key,
                redeposit_index.unwrap_or_default(),
                wallet_index.unwrap_or_default(),
            )
            .await?;
            println!("Private key: {}", key.privkey.to_hex());
            println!("Public key: {}", key.pubkey.to_hex());
        }
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferInput {
    recipient: String,
    amount: U256,
    token_index: u32,
}
