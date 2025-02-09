use std::time::Duration;

use ethers::{
    signers::coins_bip39::{English, Mnemonic},
    types::H256,
};
use intmax2_cli::cli::{
    error::CliError,
    get::balance,
    send::{transfer, TransferInput},
};
use intmax2_client_sdk::client::{
    error::ClientError, key_from_eth::generate_intmax_account_from_eth_key,
    strategy::error::StrategyError, sync::error::SyncError,
};
use intmax2_interfaces::api::error::ServerError;
use intmax2_zkp::common::signature::key_set::KeySet;
use serde::Deserialize;
use tiny_hderive::bip32::ExtendedPrivKey;

#[derive(Debug, Clone, Deserialize)]
pub struct EnvVar {
    pub master_mnemonic: String,
    pub private_key: String,
    pub num_of_recipients: Option<u32>,
    pub recipient_offset: Option<u32>,
    pub balance_prover_base_url: String,
}

#[derive(Debug, Clone, Default)]
pub struct MnemonicToPrivateKeyOptions {
    pub account_index: u32,
    pub address_index: u32,
}

pub fn mnemonic_to_private_key(
    mnemonic_phrase: &str,
    options: MnemonicToPrivateKeyOptions,
) -> Result<H256, Box<dyn std::error::Error>> {
    let mnemonic = Mnemonic::<English>::new_from_phrase(mnemonic_phrase)?;
    let seed = mnemonic.to_seed(None)?;

    let account_index = options.account_index;
    let address_index = options.address_index;
    let hd_path = format!("m/44'/60'/{account_index}'/0/{address_index}");

    let master_key = ExtendedPrivKey::derive(&seed, hd_path.as_str())
        .map_err(|e| format!("Failed to derive private key: {e:?}"))?;
    let private_key_bytes = master_key.secret();

    Ok(H256(private_key_bytes))
}

pub async fn wait_for_balance_synchronization(
    intmax_sender: KeySet,
    retry_interval: Duration,
) -> Result<(), CliError> {
    loop {
        let result = balance(intmax_sender).await;
        match result {
            Ok(_) => return Ok(()),
            Err(CliError::SyncError(SyncError::StrategyError(StrategyError::PendingTxError(
                _,
            )))) => {
                println!("Pending transaction. Waiting for the balance to be updated...");
            }
            Err(CliError::SyncError(SyncError::ValidityProverIsNotSynced(_))) => {
                println!("Validity prover is not synced. Waiting for the balance to be updated...");
            }
            Err(CliError::SyncError(SyncError::ServerError(ServerError::ServerError(
                500,
                message,
                _,
                _,
            )))) => {
                println!("{message}. Waiting for the balance to be updated...");
            }
            Err(CliError::ClientError(ClientError::ServerError(ServerError::ServerError(
                status,
                message,
                url,
                query,
            )))) => {
                println!("Server error status={status}, url={url}, query={query}");
                println!("{message}. Waiting for the balance to be updated...");
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
    num_loops: usize,
) -> Result<(), CliError> {
    for i in 0..num_loops {
        log::trace!(
            "Starting transfer from {} (iteration {}/{})",
            key.pubkey,
            i + 1,
            num_loops
        );
        transfer(key, transfer_inputs).await?;
        tokio::time::sleep(Duration::from_secs(20)).await;
        wait_for_balance_synchronization(key, Duration::from_secs(5)).await?;
    }

    Ok(())
}

pub fn derive_intmax_keys(
    master_mnemonic_phrase: &str,
    num_of_keys: u32,
    offset: u32,
) -> Result<Vec<KeySet>, Box<dyn std::error::Error>> {
    let mut intmax_senders = vec![];
    for address_index in 0..num_of_keys {
        let options = MnemonicToPrivateKeyOptions {
            account_index: 0,
            address_index: offset + address_index,
        };
        let private_key = mnemonic_to_private_key(master_mnemonic_phrase, options)?;
        let key = generate_intmax_account_from_eth_key(private_key);
        intmax_senders.push(key);
    }

    Ok(intmax_senders)
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
