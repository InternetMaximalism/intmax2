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
        key_derivation::derive_spend_key_from_eth,
        send::send_transfers,
        sync::{resync, sync_claims, sync_withdrawals},
        withdrawal::send_withdrawal,
    },
    format::{format_token_info, privkey_to_keypair},
};
use intmax2_client_sdk::client::{config::network_from_env, types::{GenericRecipient, TransferRequest}};
use intmax2_interfaces::utils::{
    address::IntmaxAddress,
    key::{KeyPair, ViewPair},
    key_derivation::{derive_keypair_from_spend_key, derive_spend_key_from_bytes32},
    random::default_rng,
};
use intmax2_zkp::ethereum_types::{bytes32::Bytes32, u32limb_trait::U32LimbTrait};

// const MAX_BATCH_TRANSFER: usize = 63;

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
            description,
            fee_token_index,
            wait,
        } => {
            let key_pair: intmax2_interfaces::utils::key::KeyPair = privkey_to_keypair(private_key);
            let transfer_request = TransferRequest {
                recipient: GenericRecipient::IntmaxAddress(to),
                amount,
                token_index,
                description,
            };
            send_transfers(
                key_pair,
                &[transfer_request],
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
            description,
            fee_token_index,
            with_claim_fee,
            wait,
        } => {
            let key_pair = privkey_to_keypair(private_key);
            let fee_token_index = fee_token_index.unwrap_or(0);
            send_withdrawal(
                key_pair,
                to,
                amount,
                token_index,
                description,
                fee_token_index,
                with_claim_fee,
                wait,
            )
            .await?;
        }
        // Commands::BatchTransfer {
        //     private_key,
        //     csv_path,
        //     fee_token_index,
        //     wait,
        // } => {
        //     let key_pair = privkey_to_keypair(private_key);
        //     let mut reader = csv::Reader::from_path(csv_path)?;
        //     let mut transfers = vec![];
        //     for result in reader.deserialize() {
        //         let transfer_input: TransferInput = result?;
        //         transfers.push(Transfer {
        //             recipient: parse_generic_address(&transfer_input.recipient)
        //                 .map_err(|e| CliError::ParseError(e.to_string()))?,
        //             amount: transfer_input.amount,
        //             token_index: transfer_input.token_index,
        //             salt: generate_salt(),
        //         });
        //     }
        //     if transfers.len() > MAX_BATCH_TRANSFER {
        //         return Err(CliError::TooManyTransfer(transfers.len()));
        //     }
        //     send_transfers(
        //         key_pair,
        //         &transfers,
        //         vec![],
        //         fee_token_index.unwrap_or_default(),
        //         wait,
        //     )
        //     .await?;
        // }
        Commands::Deposit {
            eth_private_key,
            private_key,
            amount,
            token_type,
            token_address,
            token_id,
            mining,
        } => {
            let key_pair = privkey_to_keypair(private_key);
            let (amount, token_address, token_id) =
                format_token_info(token_type, amount, token_address, token_id)?;
            deposit(
                key_pair.into(),
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
            let key_pair = privkey_to_keypair(private_key);
            sync_withdrawals(key_pair.into(), fee_token_index).await?;
        }
        Commands::SyncClaims {
            private_key,
            recipient,
            fee_token_index,
        } => {
            let key_pair = privkey_to_keypair(private_key);
            sync_claims(key_pair.into(), recipient, fee_token_index).await?;
        }
        Commands::ClaimBuilderReward { eth_private_key } => {
            claim_builder_reward(eth_private_key).await?;
        }
        Commands::Balance {
            private_key,
            without_sync,
        } => {
            let key_pair = privkey_to_keypair(private_key);
            balance(key_pair.into(), !without_sync).await?;
        }
        Commands::UserData { private_key } => {
            let key_pair = privkey_to_keypair(private_key);
            get_user_data(key_pair.into()).await?;
        }
        Commands::History {
            private_key,
            order,
            from,
        } => {
            let key_pair = privkey_to_keypair(private_key);
            let order = order.unwrap_or_default();
            history(key_pair.into(), order, from).await?;
        }
        Commands::WithdrawalStatus { private_key } => {
            let key_pair = privkey_to_keypair(private_key);
            withdrawal_status(key_pair.into()).await?;
        }
        Commands::MiningList { private_key } => {
            let key_pair = privkey_to_keypair(private_key);
            mining_list(key_pair.into()).await?;
        }
        Commands::ClaimStatus { private_key } => {
            let key_pair = privkey_to_keypair(private_key);
            claim_status(key_pair.into()).await?;
        }
        Commands::PaymentMemos { private_key, name } => {
            let key_pair = privkey_to_keypair(private_key);
            get_payment_memos(key_pair.into(), &name).await?;
        }
        Commands::ClaimWithdrawals {
            private_key,
            eth_private_key,
        } => {
            let key_pair = privkey_to_keypair(private_key);
            claim_withdrawals(key_pair.into(), eth_private_key).await?;
        }
        Commands::Resync { private_key, deep } => {
            let key_pair = privkey_to_keypair(private_key);
            resync(key_pair.into(), deep).await?;
        }
        Commands::MakeBackup {
            private_key,
            dir,
            from,
        } => {
            let key_pair = privkey_to_keypair(private_key);
            let from = from.unwrap_or_default();
            let dir = dir.unwrap_or_default();
            make_history_backup(key_pair.into(), &dir, from).await?;
        }
        Commands::IncorporateBackup { path } => {
            incorporate_backup(&path)?;
        }
        Commands::CheckValidityProver => {
            check_validity_prover().await?;
        }
        Commands::GenerateKey => {
            let mut rng = default_rng();
            let input = Bytes32::rand(&mut rng);
            let spend_key = derive_spend_key_from_bytes32(input);
            let key_pair = derive_keypair_from_spend_key(spend_key, false);
            print_keys(key_pair);
        }
        Commands::PublicKey { private_key } => {
            let key_pair: KeyPair = privkey_to_keypair(private_key);
            print_keys(key_pair);
        }
        Commands::KeyFromBackupKey { backup_key } => {
            let spend_key = derive_spend_key_from_bytes32(backup_key);
            let key_pair = derive_keypair_from_spend_key(spend_key, false);
            print_keys(key_pair);
        }
        Commands::KeyFromEth {
            eth_private_key,
            redeposit_index,
            wallet_index,
        } => {
            let spend_key = derive_spend_key_from_eth(
                eth_private_key,
                redeposit_index.unwrap_or_default(),
                wallet_index.unwrap_or_default(),
            )
            .await?;
            let key_pair = derive_keypair_from_spend_key(spend_key, false);
            print_keys(key_pair);
        }
    }
    Ok(())
}

fn print_keys(key_pair: KeyPair) {
    let network = network_from_env();
    let view_pair: ViewPair = key_pair.into();
    let address = IntmaxAddress::from_viewpair(network, &view_pair);
    println!("Address: {}", address.to_string().green());
    println!("View Only Key: {}", view_pair.to_string().green());
    println!("Spend Key: {}", key_pair.spend.to_string().green());
}
