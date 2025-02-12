use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use ethers::{types::H256, utils::hex};
use futures::future::join_all;
use intmax2_cli::{
    cli::{
        error::CliError,
        send::{transfer, TransferInput},
    },
    format::privkey_to_keyset,
};
use intmax2_client_sdk::external_api::utils::query::get_request;
use intmax2_zkp::{
    common::signature::key_set::KeySet, ethereum_types::u32limb_trait::U32LimbTrait,
};
use serde::Deserialize;
use tests::{derive_intmax_keys, transfer_with_error_handling};

#[derive(Debug, Clone, Deserialize)]
pub struct EnvVar {
    pub master_mnemonic: String,
    pub private_key: String,
    pub num_of_recipients: Option<u32>,
    pub recipient_offset: Option<u32>,
    pub balance_prover_base_url: String,
    // pub cool_down_seconds: Option<u64>,
}

const ETH_TOKEN_INDEX: u32 = 0;

// pub async fn process_account(key: KeySet, transfers: &[TransferInput]) -> Result<(), CliError> {
//     wait_for_balance_synchronization(key, Duration::from_secs(5)).await?;
//     transfer(key, transfers, ETH_TOKEN_INDEX).await?;
//     tokio::time::sleep(Duration::from_secs(20)).await;
//     wait_for_balance_synchronization(key, Duration::from_secs(5)).await?;

//     Ok(())
// }

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let current_dir = std::env::current_dir()?;
    let env_path = current_dir.join("./tests/.env");
    println!("env_path: {}", env_path.to_string_lossy());
    dotenv::from_path(env_path)?;
    let cli_env_path = current_dir.join("./cli/.env");
    println!("cli_env_path: {}", cli_env_path.to_string_lossy());
    dotenv::from_path(cli_env_path)?;
    let config = envy::from_env::<EnvVar>().unwrap();
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .init();

    let my_var = std::env::var("ENV").expect("MY_ENV_VAR not found");
    println!("ENV: {}", my_var);

    log::info!("Start soak test");
    let private_key = H256::from_slice(&hex::decode(config.private_key)?);
    let admin_key = privkey_to_keyset(private_key);

    let system = TestSystem::new(admin_key, config.master_mnemonic);
    system.run_soak_test().await?;
    log::info!("End soak test");

    Ok(())
}

#[derive(Debug, Clone, Deserialize)]
struct TestConfig {
    concurrent_limit: usize,
    end: String,
}

async fn get_config() -> Result<TestConfig, Box<dyn std::error::Error>> {
    let config = get_request::<(), TestConfig>("http://localhost:8080", "/config", None).await?;

    Ok(config)
}

#[derive(Debug, Clone)]
struct TestSystem {
    admin_key: KeySet,
    master_mnemonic_phrase: String,
    accounts: Arc<Mutex<Vec<KeySet>>>,
}

impl TestSystem {
    pub fn new(admin_key: KeySet, master_mnemonic_phrase: String) -> Self {
        Self {
            admin_key,
            master_mnemonic_phrase,
            accounts: Arc::new(Mutex::new(Vec::new())),
        }
    }

    async fn transfer_from(
        &self,
        sender: KeySet,
        intmax_recipients: &[KeySet],
        amount: u128,
    ) -> Result<(), CliError> {
        let transfers = intmax_recipients
            .iter()
            .map(|recipient| TransferInput {
                recipient: recipient.pubkey.to_hex(),
                amount, // 1000000000u128,
                token_index: ETH_TOKEN_INDEX,
            })
            .collect::<Vec<_>>();
        transfer(sender, &transfers, ETH_TOKEN_INDEX).await
    }

    async fn ensure_accounts(
        &self,
        required_count: usize,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let accounts_len = self.accounts.lock().unwrap().len();

        if accounts_len >= required_count {
            return Ok(());
        }
        let num_of_keys = required_count - accounts_len;

        // Create new account and receive initial balance from admin
        let master_mnemonic_phrase = self.master_mnemonic_phrase.clone();
        let new_accounts = derive_intmax_keys(
            &master_mnemonic_phrase,
            num_of_keys as u32,
            accounts_len as u32,
        )?;

        // TODO: Make it so that each of the 63 addresses that received ETH sends it to the other 63 addresses.
        let chunk_size = 63;
        for chunk in new_accounts.chunks(chunk_size) {
            self.transfer_from(self.admin_key, chunk, 1000000000u128)
                .await?;
            self.accounts.lock().unwrap().extend(chunk.iter());
        }

        Ok(())
    }

    pub async fn run_soak_test(&self) -> Result<(), Box<dyn std::error::Error>> {
        loop {
            let config = get_config().await?;
            println!("Concurrency: {}", config.concurrent_limit);
            if config.end == "true" {
                break;
            }

            let concurrent_limit = config.concurrent_limit;

            // Ensure we have enough accounts
            self.ensure_accounts(concurrent_limit).await?;

            let intmax_senders = self.accounts.lock().unwrap()[0..concurrent_limit].to_vec();

            let futures = intmax_senders
                .iter()
                .enumerate()
                .map(|(i, sender)| async move {
                    let transfers = [TransferInput {
                        recipient: self.admin_key.pubkey.to_hex(),
                        amount: 10u128,
                        token_index: ETH_TOKEN_INDEX,
                    }];
                    let res = transfer_with_error_handling(*sender, &transfers, 2)
                        .await
                        .err();
                    log::info!("Transfer completed from {} (No.{})", sender.pubkey, i);
                    res
                });

            println!("Starting transactions");
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(900)) => log::info!("transaction timeout"),
                errors = join_all(futures) => {
                    for (i, error) in errors.iter().enumerate() {
                        if let Some(e) = error {
                            log::error!("Recipient ({}/{}) failed: {:?}", i + 1, concurrent_limit, e);
                        } else {
                            log::info!("Recipient ({}/{}) succeeded", i + 1, concurrent_limit);
                        }
                    }

                    log::info!("Completed transactions");
                },
            }
        }

        Ok(())
    }
}
