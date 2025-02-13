use intmax2_cli::cli::{
    error::CliError,
    get::{balance, BalanceInfo},
    send::{transfer, TransferInput},
};
use intmax2_client_sdk::client::{
    error::ClientError, strategy::error::StrategyError, sync::error::SyncError,
};
use intmax2_interfaces::api::error::ServerError;
use intmax2_zkp::common::signature::key_set::KeySet;
use serde::Deserialize;
use std::time::Duration;

pub mod accounts;

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
    transfer_inputs: &[TransferInput],
) -> Result<(), CliError> {
    let timer = std::time::Instant::now();
    transfer(key, transfer_inputs, 0).await?;
    log::info!(
        "Complete transfer from {} ({} s)",
        key.pubkey,
        timer.elapsed().as_secs()
    );
    tokio::time::sleep(Duration::from_secs(20)).await;
    wait_for_balance_synchronization(key, Duration::from_secs(5))
        .await
        .map_err(|err| {
            println!("transfer_with_error_handling Error: {:?}", err);
            err
        })?;

    Ok(())
}

pub async fn withdraw_with_error_handling(
    key: KeySet,
    transfer_inputs: &[TransferInput],
    num_loops: usize,
) -> Result<(), CliError> {
    for i in 0..num_loops {
        log::info!(
            "Starting transfer from {} (iteration {}/{})",
            key.pubkey,
            i + 1,
            num_loops
        );
        let timer = std::time::Instant::now();
        transfer(key, transfer_inputs, 0).await?;
        log::info!(
            "Complete transfer from {} ({} s)",
            key.pubkey,
            timer.elapsed().as_secs()
        );
        tokio::time::sleep(Duration::from_secs(20)).await;
        wait_for_balance_synchronization(key, Duration::from_secs(5))
            .await
            .map_err(|err| {
                println!("transfer_with_error_handling Error: {:?}", err);
                err
            })?;
    }

    Ok(())
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
