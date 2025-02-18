use ethers::types::H256;
use intmax2_cli::cli::{
    deposit::deposit,
    error::CliError,
    get::{balance, BalanceInfo},
    send::send_transfers,
    sync::sync_withdrawals,
};
use intmax2_client_sdk::{
    client::{error::ClientError, strategy::error::StrategyError, sync::error::SyncError},
    external_api::utils::retry::{retry_if, RetryConfig},
};
use intmax2_interfaces::{api::error::ServerError, data::deposit_data::TokenType};
use intmax2_zkp::{
    common::{generic_address::GenericAddress, signature::key_set::KeySet, transfer::Transfer},
    ethereum_types::{address::Address, u256::U256, u32limb_trait::U32LimbTrait},
};
use serde::Deserialize;
use std::time::Duration;

pub mod accounts;
pub mod ethereum;

// dev environment
const TRANSFER_WAITING_DURATION: u64 = 20;
const TRANSFER_POLLING_DURATION: u64 = 5;
const DEPOSIT_WAITING_DURATION: u64 = 20 * 60;
const DEPOSIT_POLLING_DURATION: u64 = 60;
// const WITHDRAWAL_WAITING_DURATION: u64 = 120;
// const WITHDRAWAL_POLLING_DURATION: u64 = 10;

#[derive(Debug, Clone, Deserialize)]
pub struct EnvVar {
    pub master_mnemonic: String,
    pub private_key: String,
    pub num_of_recipients: Option<u32>,
    pub recipient_offset: Option<u32>,
    pub balance_prover_base_url: String,
}

pub async fn wait_for_balance_synchronization(
    key: KeySet,
    retry_interval: Duration,
) -> Result<Vec<BalanceInfo>, CliError> {
    loop {
        let timer = std::time::Instant::now();
        let result = balance(key).await;
        match result {
            Ok(balances) => {
                log::info!(
                    "Sync balance from {} ({} s)",
                    key.pubkey,
                    timer.elapsed().as_secs()
                );
                return Ok(balances);
            }
            Err(CliError::ClientError(ClientError::StrategyError(
                StrategyError::PendingTxError(_),
            ))) => {
                log::warn!("Pending transaction. Waiting for the balance to be updated...");
            }
            Err(CliError::SyncError(SyncError::StrategyError(StrategyError::PendingTxError(
                _,
            )))) => {
                log::warn!("Pending transaction. Waiting for the balance to be updated...");
            }
            Err(CliError::SyncError(SyncError::ValidityProverIsNotSynced(_))) => {
                log::warn!(
                    "Validity prover is not synced. Waiting for the balance to be updated..."
                );
            }
            Err(CliError::SyncError(SyncError::ServerError(ServerError::ServerError(
                500,
                message,
                _,
                _,
            )))) => {
                log::warn!("{message}. Waiting for the balance to be updated...");
            }
            Err(CliError::SyncError(SyncError::ServerError(ServerError::ServerError(
                503,
                message,
                _,
                _,
            )))) => {
                log::warn!("{message}. Waiting for the balance to be updated...");

                // Wait for an additional minute
                tokio::time::sleep(Duration::from_secs(60)).await;
            }
            Err(CliError::ClientError(ClientError::ServerError(ServerError::ServerError(
                status,
                message,
                url,
                query,
            )))) => {
                log::error!("Server error status={status}, url={url}, query={query}");
                log::error!("{message}. Waiting for the balance to be updated...");
            }
            Err(e) => {
                return Err(e);
            }
        }

        tokio::time::sleep(retry_interval).await;
    }
}

pub async fn transfer_with_error_handling(
    key: KeySet,
    transfer_inputs: &[Transfer],
) -> Result<(), CliError> {
    for transfer_input in transfer_inputs {
        if !transfer_input.recipient.is_pubkey {
            return Err(CliError::ParseError(
                "Invalid recipient INTMAX address".to_string(),
            ));
        }
    }

    let timer = std::time::Instant::now();
    send_transfers(key, transfer_inputs, vec![], 0).await?;
    log::info!(
        "Complete transfer from {} ({} s)",
        key.pubkey,
        timer.elapsed().as_secs()
    );

    tokio::time::sleep(Duration::from_secs(TRANSFER_WAITING_DURATION)).await;
    wait_for_balance_synchronization(key, Duration::from_secs(TRANSFER_POLLING_DURATION))
        .await
        .map_err(|err| {
            println!("transfer_with_error_handling Error: {:?}", err);
            err
        })?;

    Ok(())
}

pub async fn deposit_native_token_with_error_handling(
    depositor_eth_private_key: H256,
    recipient_key: KeySet,
    amount: U256,
) -> Result<(), CliError> {
    let token_type = TokenType::NATIVE;
    // let amount = ethers::types::U256::from(10);
    let token_address = ethers::types::Address::default();
    let token_id = U256::from(0);
    let is_mining = false;

    let timer = std::time::Instant::now();
    deposit(
        recipient_key,
        depositor_eth_private_key,
        token_type,
        amount,
        token_address,
        token_id,
        is_mining,
    )
    .await?;
    log::info!(
        "Complete transfer from {} ({} s)",
        recipient_key.pubkey,
        timer.elapsed().as_secs()
    );

    // Wait for messaging to Scroll network
    tokio::time::sleep(Duration::from_secs(DEPOSIT_WAITING_DURATION)).await;

    wait_for_balance_synchronization(recipient_key, Duration::from_secs(DEPOSIT_POLLING_DURATION))
        .await
        .map_err(|err| {
            println!("deposit_native_token_with_error_handling Error: {:?}", err);
            err
        })?;

    Ok(())
}

pub async fn withdraw_directly_with_error_handling(
    key: KeySet,
    withdrawal_input: Transfer,
) -> Result<(), CliError> {
    if withdrawal_input.recipient.is_pubkey {
        return Err(CliError::ParseError(format!(
            "Invalid recipient Ethereum address: {}",
            withdrawal_input.recipient.data.to_hex()
        )));
    }

    let retry_config = RetryConfig {
        max_retries: 100,
        initial_delay: 10000,
    };
    let transfer_inputs = &[withdrawal_input];
    retry_if(
        |_: &CliError| true,
        || send_transfers(key, transfer_inputs, vec![], 0),
        retry_config,
    )
    .await?;

    tokio::time::sleep(Duration::from_secs(TRANSFER_WAITING_DURATION)).await;
    wait_for_balance_synchronization(key, Duration::from_secs(TRANSFER_POLLING_DURATION))
        .await
        .map_err(|err| {
            println!(
                "withdraw_native_token_with_error_handling before sync_withdrawals Error: {:?}",
                err
            );
            err
        })?;

    retry_if(
        |_: &CliError| true,
        || sync_withdrawals(key, Some(0)),
        retry_config,
    )
    .await?;

    Ok(())
}

pub fn mul_u256(amount: U256, max_transfers_per_transaction: usize, num_accounts: usize) -> U256 {
    let amount_big = num_bigint::BigUint::from_bytes_be(&amount.to_bytes_be());
    let max_transfers_per_transaction_big =
        num_bigint::BigUint::from(max_transfers_per_transaction);
    let num_accounts_big = num_bigint::BigUint::from(num_accounts);
    let amount_big = amount_big * max_transfers_per_transaction_big * num_accounts_big;

    // validation for overflow
    assert!(amount_big.bits() <= 256);

    U256::from_bytes_be(&amount_big.to_bytes_be())
}

pub fn address_to_generic_address(eth_address: ethers::types::Address) -> GenericAddress {
    GenericAddress::from_address(Address::from_bytes_be(eth_address.as_bytes()))
}

// const NUM_TRANSFER_LOOPS: usize = 2;
// async fn account_loop(
//     stats: Arc<RwLock<HashMap<String, TransactionStats>>>,
//     semaphore: Arc<Semaphore>,
//     account_id: u32,
//     key: KeySet,
//     transfers: TransferWrapper,
//     cool_down_seconds: u64,
//     num_of_recipients: u32,
// ) {
//     loop {
//         if account_id >= num_of_recipients {
//             continue;
//         }

//         let permit = semaphore.clone().acquire_owned().await.unwrap();
//         log::trace!("Starting transfer from {}", key.pubkey);
//         let result = process_account(key, &transfers.transfers).await;

//         {
//             // let mut stats = stats.lock().unwrap();
//             let mut stats_write = stats.write().unwrap();
//             let entry = stats_write
//                 .entry(key.pubkey.to_hex())
//                 .or_insert(TransactionStats::default());

//             match result {
//                 Ok(_) => *entry.success_count.get_mut() += 1,
//                 Err(_) => *entry.failure_count.get_mut() += 1,
//             }
//         }

//         drop(permit);

//         println!(
//             "[Account {}] Cooldown: Waiting {} seconds before next transaction...\n",
//             key.pubkey.to_hex(),
//             cool_down_seconds
//         );
//         tokio::time::sleep(Duration::from_secs(cool_down_seconds)).await;
//     }
// }
