use futures::future::join_all;
use intmax2_cli::cli::{
    client::get_client,
    deposit::deposit,
    error::CliError,
    get::{balance, BalanceInfo},
    send::send_transfers,
    sync::sync_withdrawals,
    withdrawal::send_withdrawal,
};
use intmax2_client_sdk::{
    client::{
        error::ClientError,
        history::{EntryStatus, HistoryEntry},
        strategy::error::StrategyError,
        sync::error::SyncError,
    },
    external_api::utils::retry::{retry_if, RetryConfig},
};
use intmax2_interfaces::{
    api::{
        error::ServerError,
        store_vault_server::types::{CursorOrder, MetaDataCursor},
        withdrawal_server::interface::WithdrawalStatus,
    },
    data::{
        deposit_data::{DepositData, TokenType},
        meta_data::MetaData,
    },
};
use intmax2_zkp::{
    common::{
        generic_address::GenericAddress, signature_content::key_set::KeySet, transfer::Transfer,
    },
    ethereum_types::{address::Address, bytes32::Bytes32, u256::U256, u32limb_trait::U32LimbTrait},
};
use serde::Deserialize;
use std::{future::Future, time::Duration};

pub mod accounts;
pub mod ethereum;
pub mod task;

// dev environment
const DEPOSIT_WAITING_DURATION: u64 = 20 * 60;
const DEPOSIT_POLLING_DURATION: u64 = 60;
const ETH_TOKEN_INDEX: u32 = 0;

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
        let result = balance(key, true).await;
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
    send_transfers(key, transfer_inputs, vec![], 0, true).await?;
    log::info!(
        "Complete transfer from {} ({} s)",
        key.pubkey,
        timer.elapsed().as_secs()
    );

    // tokio::time::sleep(Duration::from_secs(TRANSFER_WAITING_DURATION)).await;
    // wait_for_balance_synchronization(key, Duration::from_secs(TRANSFER_POLLING_DURATION))
    //     .await
    //     .map_err(|err| {
    //         println!("transfer_with_error_handling Error: {:?}", err);
    //         err
    //     })?;

    Ok(())
}

pub async fn deposit_native_token_with_error_handling(
    depositor_eth_private_key: Bytes32,
    recipient_key: KeySet,
    amount: U256,
    waiting_duration: Option<Duration>,
    is_mining: bool,
) -> Result<(), CliError> {
    let token_type = TokenType::NATIVE;
    // let amount = ethers::types::U256::from(10);
    let token_address = Address::default();
    let token_id = U256::from(0);

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
        "Complete deposit from {} ({} s)",
        recipient_key.pubkey,
        timer.elapsed().as_secs()
    );

    // Wait for messaging to Scroll network
    log::info!("Waiting for messaging to Scroll network...");
    tokio::time::sleep(waiting_duration.unwrap_or(Duration::from_secs(DEPOSIT_WAITING_DURATION)))
        .await;

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
    to: Address,
    amount: U256,
    token_index: u32,
    with_claim_fee: bool,
) -> Result<(), CliError> {
    // First attempt with the requested amount
    let mut result =
        withdraw_directly_with_error_handling_inner(key, to, amount, token_index, with_claim_fee)
            .await;

    // Handle fee payment if needed
    if matches!(result, Err(CliError::SyncError(SyncError::FeeError(_)))) {
        log::warn!("There is an unpaid fee.");

        // Pay pending fees by sending zero-amount transactions until successful
        while let Err(CliError::SyncError(SyncError::FeeError(_))) = result {
            result = withdraw_directly_with_error_handling_inner(
                key,
                to,
                U256::default(),
                token_index,
                with_claim_fee,
            )
            .await;
        }

        // If fee payment failed with a different error, return that error
        result?;

        // Retry the original withdrawal
        result = withdraw_directly_with_error_handling_inner(
            key,
            to,
            amount,
            token_index,
            with_claim_fee,
        )
        .await;
    }

    result
}

async fn withdraw_directly_with_error_handling_inner(
    key: KeySet,
    to: Address,
    amount: U256,
    token_index: u32,
    with_claim_fee: bool,
) -> Result<(), CliError> {
    let retry_config = RetryConfig {
        max_retries: 100,
        initial_delay: 10000,
    };
    let retry_condition = |_: &CliError| true;
    retry_if(
        retry_condition,
        || {
            send_withdrawal(
                key,
                to,
                amount,
                token_index,
                ETH_TOKEN_INDEX,
                with_claim_fee,
                true,
            )
        },
        retry_config,
    )
    .await?;

    let retry_config = RetryConfig {
        max_retries: 5,
        initial_delay: 10000,
    };
    let retry_condition =
        |err: &CliError| !matches!(err, CliError::SyncError(SyncError::FeeError(_)));
    retry_if(
        retry_condition,
        || sync_withdrawals(key, Some(0)),
        retry_config,
    )
    .await?;

    Ok(())
}

pub async fn wait_for_withdrawal_finalization(key: KeySet) -> Result<(), CliError> {
    let client = get_client()?;
    let mut withdrawal_info = client.get_withdrawal_info(key).await?;
    let mut pending_withdrawals = withdrawal_info
        .into_iter()
        .filter(|withdrawal_info| {
            withdrawal_info.status == WithdrawalStatus::Requested
                || withdrawal_info.status == WithdrawalStatus::Relayed
        })
        .collect::<Vec<_>>();

    const WITHDRAWAL_WAITING_RETRY_TIMES: usize = 40;
    const NUM_PENDING_WITHDRAWALS: usize = 30;
    let mut retries = 0;
    while !pending_withdrawals.is_empty() {
        if retries > WITHDRAWAL_WAITING_RETRY_TIMES {
            if pending_withdrawals.len() < NUM_PENDING_WITHDRAWALS {
                log::error!(
                    "Failed to finalize withdrawal after {} retries",
                    WITHDRAWAL_WAITING_RETRY_TIMES
                );
                break;
            }

            log::warn!("Too many pending withdrawals");
        }

        log::info!("Waiting for withdrawal finalization...");
        tokio::time::sleep(Duration::from_secs(30)).await;
        withdrawal_info = client.get_withdrawal_info(key).await?;
        pending_withdrawals = withdrawal_info
            .into_iter()
            .filter(|withdrawal_info| {
                // some of the pending withdrawals are not in the withdrawal_info
                pending_withdrawals.iter().any(|pending_withdrawal| {
                    pending_withdrawal.contract_withdrawal.withdrawal_hash()
                        == withdrawal_info.contract_withdrawal.withdrawal_hash()
                })
            })
            .filter(|withdrawal_info| {
                withdrawal_info.status == WithdrawalStatus::Requested
                    || withdrawal_info.status == WithdrawalStatus::Relayed
            })
            .collect::<Vec<_>>();
        log::info!(
            "Pending withdrawals: {:?}",
            pending_withdrawals
                .iter()
                .map(|w| w.contract_withdrawal.withdrawal_hash())
                .collect::<Vec<_>>()
        );

        retries += 1;
    }

    println!("Go to next step");

    Ok(())
}

pub async fn wait_for_deposit_confirmation(key: KeySet) -> Result<(), Box<dyn std::error::Error>> {
    let client = get_client().unwrap();
    let cursor = MetaDataCursor {
        cursor: None,
        order: CursorOrder::Desc,
        limit: Some(1),
    };

    loop {
        let (deposit_history, _) = client.fetch_deposit_history(key, &cursor).await?;
        let latest_deposit = deposit_history.first().unwrap().clone();
        if let EntryStatus::Settled(_) = latest_deposit.status {
            return Ok(());
        }

        log::warn!("New deposit is not found yet. Retrying in 60 seconds");
        tokio::time::sleep(Duration::from_secs(60)).await;
    }
}

pub async fn wait_for_deposit_confirmation_with_hash(
    key: KeySet,
    latest_deposit: &HistoryEntry<DepositData>,
) -> Result<(), Box<dyn std::error::Error>> {
    let client = get_client().unwrap();
    let cursor = MetaDataCursor {
        cursor: Some(MetaData {
            timestamp: latest_deposit.meta.timestamp,
            digest: Bytes32::default(),
        }),
        order: CursorOrder::Desc,
        limit: None,
    };

    loop {
        let (deposit_history, _) = client.fetch_deposit_history(key, &cursor).await?;
        let new_latest_deposit = deposit_history.first().unwrap().clone();
        if latest_deposit.data.deposit_hash() != new_latest_deposit.data.deposit_hash() {
            if let EntryStatus::Settled(_) = new_latest_deposit.status {
                return Ok(());
            }
        }

        log::warn!("New deposit is not found yet. Retrying in 60 seconds");
        tokio::time::sleep(Duration::from_secs(60)).await;
    }
}

pub fn mul_u256(amount: U256, max_transfers_per_transaction: usize, num_accounts: usize) -> U256 {
    let amount_big = num_bigint::BigUint::from_bytes_be(&amount.to_bytes_be());
    let max_transfers_per_transaction_big =
        num_bigint::BigUint::from(max_transfers_per_transaction);
    let num_accounts_big = num_bigint::BigUint::from(num_accounts);
    let amount_big = amount_big * max_transfers_per_transaction_big * num_accounts_big;

    // validation for overflow
    assert!(amount_big.bits() <= 256);

    let amount_bytes = amount_big.to_bytes_be();
    let mut new_amount_bytes = vec![0; 32];
    new_amount_bytes[32 - amount_bytes.len()..].copy_from_slice(&amount_bytes);

    U256::from_bytes_be(&new_amount_bytes).unwrap()
}

pub fn address_to_generic_address(eth_address: ethers::types::Address) -> GenericAddress {
    GenericAddress::from(Address::from_bytes_be(eth_address.as_bytes()).unwrap())
}

pub async fn log_polling_futures<F, E>(futures: &mut Vec<F>, senders: &[KeySet])
where
    E: std::fmt::Debug,
    F: Future<Output = Result<(), E>> + Unpin,
{
    let num_using_accounts = futures.len();
    assert!(
        senders.len() >= num_using_accounts,
        "Number of senders is less than the number of futures"
    );

    let results = join_all(futures).await;

    for (i, result) in results.iter().enumerate() {
        if let Err(e) = result {
            log::error!(
                "Recipient {} ({}/{}) failed: {:?}",
                senders[i].pubkey.to_hex(),
                i + 1,
                num_using_accounts,
                e
            );
            continue;
        }

        log::info!(
            "Recipient {} ({}/{}) succeeded",
            senders[i].pubkey.to_hex(),
            i + 1,
            num_using_accounts
        );
    }
}

/// Get the balance of the specified token index for a recipient
pub async fn get_eth_balance_on_intmax(key: KeySet) -> Result<U256, Box<dyn std::error::Error>> {
    let balances = wait_for_balance_synchronization(key, Duration::from_secs(5)).await?;
    for balance_info in balances {
        if balance_info.token_index == ETH_TOKEN_INDEX {
            return Ok(balance_info.amount);
        }
    }

    // If ETH balance not found, treat as zero
    Ok(U256::from(0))
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
