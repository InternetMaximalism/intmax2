use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use ethers::{
    types::H256,
    utils::hex::{self},
};
use futures::future::join_all;
use intmax2_cli::{
    cli::{error::CliError, send::send_transfers},
    format::privkey_to_keyset,
};
use intmax2_client_sdk::{
    client::sync::utils::generate_salt, external_api::utils::query::get_request,
};
use intmax2_zkp::{
    common::{generic_address::GenericAddress, signature::key_set::KeySet, transfer::Transfer},
    ethereum_types::{u256::U256, u32limb_trait::U32LimbTrait},
};
use serde::Deserialize;
use tests::{
    accounts::{derive_withdrawal_intmax_keys, mnemonic_to_account},
    address_to_generic_address, log_polling_futures, mul_u256, wait_for_balance_synchronization,
    withdraw_directly_with_error_handling,
};

#[derive(Debug, Clone, Deserialize)]
pub struct EnvVar {
    pub master_mnemonic: String,
    pub withdrawal_admin_private_key: String, // INTMAX private key
    pub num_of_recipients: Option<u32>,
    pub recipient_offset: Option<u32>,
    pub balance_prover_base_url: String,
    pub l1_chain_id: u64,

    #[serde(default = "default_url")]
    pub config_server_base_url: String,

    #[serde(default)]
    pub eth_refill_offset: usize,
}

fn default_url() -> String {
    "0.0.0.0:8080".to_string()
}

const ETH_TOKEN_INDEX: u32 = 0;
const WITHDRAWAL_TIMEOUT: u64 = 10 * 60;

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

    log::info!("Start withdrawal soak test");
    let private_key = H256::from_slice(&hex::decode(config.withdrawal_admin_private_key)?);
    let admin_intmax_key = privkey_to_keyset(private_key);

    let config_server_base_url = config.config_server_base_url.to_string();

    let system = TestSystem::new(
        admin_intmax_key,
        config.master_mnemonic,
        config_server_base_url,
        config.eth_refill_offset,
    );
    system.run_soak_test().await?;
    log::info!("End withdrawal soak test");

    Ok(())
}

#[derive(Debug, Clone, Deserialize)]
struct TestConfig {
    concurrent_limit: usize,
    end: String,
}

async fn get_config(base_url: &str) -> Result<TestConfig, Box<dyn std::error::Error>> {
    let config = get_request::<(), TestConfig>(base_url, "/config", None).await?;

    Ok(config)
}

#[derive(Debug, Clone)]
struct TestSystem {
    admin_intmax_key: KeySet,
    master_mnemonic_phrase: String,
    config_server_base_url: String,
    accounts: Arc<Mutex<Vec<KeySet>>>,
    eth_refill_offset: usize,
}

impl TestSystem {
    pub fn new(
        admin_intmax_key: KeySet,
        master_mnemonic_phrase: String,
        config_server_base_url: String,
        eth_refill_offset: usize,
    ) -> Self {
        Self {
            admin_intmax_key,
            master_mnemonic_phrase,
            accounts: Arc::new(Mutex::new(Vec::new())),
            config_server_base_url,
            eth_refill_offset,
        }
    }

    async fn transfer_from(
        &self,
        sender: KeySet,
        intmax_recipients: &[KeySet],
        amount: U256,
    ) -> Result<(), CliError> {
        wait_for_balance_synchronization(sender, Duration::from_secs(5)).await?;
        let transfers = intmax_recipients
            .iter()
            .map(|recipient| Transfer {
                recipient: GenericAddress::from_pubkey(recipient.pubkey),
                amount, // 1000000000u128,
                token_index: ETH_TOKEN_INDEX,
                salt: generate_salt(),
            })
            .collect::<Vec<_>>();
        send_transfers(sender, &transfers, vec![], ETH_TOKEN_INDEX, true).await
    }

    async fn ensure_accounts_without_transfers(
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
        let new_accounts = derive_withdrawal_intmax_keys(
            &master_mnemonic_phrase,
            num_of_keys as u32,
            accounts_len as u32,
        )?;

        let chunk_size = 63;
        for chunk in new_accounts.chunks(chunk_size) {
            self.accounts.lock().unwrap().extend(chunk.iter());
        }

        Ok(())
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
        let new_accounts = derive_withdrawal_intmax_keys(
            &master_mnemonic_phrase,
            num_of_keys as u32,
            accounts_len as u32,
        )?;

        // TODO: Make it so that each of the 63 addresses that received ETH sends it to the other 63 addresses.
        // let chunk_size = 63;
        // for chunk in new_accounts.chunks(chunk_size) {
        //     self.transfer_from(self.admin_key, chunk, 1000000000u128)
        //         .await?;
        //     self.accounts.lock().unwrap().extend(chunk.iter());
        // }

        let amount = U256::from(1000000000);
        let chunk_size = 63;
        let results = self.distribute(&new_accounts, amount, chunk_size).await?;
        for err in results.into_iter().flatten() {
            log::error!("Failed to distribute balance: {:?}", err)
        }

        Ok(())
    }

    /// Distribute the given amount to the accounts.
    async fn distribute(
        &self,
        accounts: &[KeySet],
        amount: U256,
        max_transfers_per_transaction: usize,
    ) -> Result<Vec<Option<CliError>>, Box<dyn std::error::Error>> {
        log::info!("admin pubkey: {}", self.admin_intmax_key.pubkey.to_hex());
        if accounts.len() <= max_transfers_per_transaction {
            self.transfer_from(self.admin_intmax_key, accounts, amount)
                .await?;
            self.accounts.lock().unwrap().extend(accounts.iter());

            return Ok(vec![]);
        }

        // Split new_accounts into two parts: intermediates and rest
        let (intermediates, rest) = accounts.split_at(max_transfers_per_transaction);
        let amount_for_intermediates =
            mul_u256(amount, accounts.len(), max_transfers_per_transaction);

        // Transfer from admin to intermediates
        log::info!("Transfer from admin to intermediates");
        self.transfer_from(
            self.admin_intmax_key,
            intermediates,
            amount_for_intermediates,
        )
        .await?;
        self.accounts.lock().unwrap().extend(intermediates.iter());

        // Distribute `rest` into `chunk_size` groups
        let mut groups: Vec<Vec<KeySet>> = vec![Vec::new(); max_transfers_per_transaction];
        for (i, key) in rest.iter().enumerate() {
            groups[i % max_transfers_per_transaction].push(*key);
        }

        // Transfer from intermediates to rest
        let transfers = groups
            .iter()
            .zip(intermediates)
            .map(|(group, sender)| async move {
                for chunk in group.chunks(max_transfers_per_transaction) {
                    self.transfer_from(*sender, chunk, amount).await?;
                    self.accounts.lock().unwrap().extend(chunk.iter());
                }

                Ok::<(), CliError>(())
            });

        log::info!("Transfer from intermediates to rest");
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(600)) => Ok(vec![]), // Err("transaction timeout".into()),
            results = join_all(transfers) => {
                let res = results
                    .into_iter()
                    .map(|result| result.err())
                    .collect::<Vec<_>>();

                Ok(res)
            }
        }
    }

    pub async fn run_soak_test(&self) -> Result<(), Box<dyn std::error::Error>> {
        let trash_account = mnemonic_to_account(&self.master_mnemonic_phrase, 1, 0)?;
        let timeout_duration = Duration::from_secs(WITHDRAWAL_TIMEOUT);

        // NOTE: Be cautious of insufficient balances, as funds will only be sent once to the intermediary address.
        // self.ensure_accounts_without_transfers(4800).await?;
        log::info!("eth_refill_offset: {}", self.eth_refill_offset);
        if self.eth_refill_offset != 0 {
            self.ensure_accounts_without_transfers(self.eth_refill_offset)
                .await?;
        }

        loop {
            let config = get_config(&self.config_server_base_url).await?;
            log::info!("Concurrency: {}", config.concurrent_limit);
            if config.end == "true" {
                break;
            }

            let concurrent_limit = config.concurrent_limit;

            // Ensure we have enough accounts
            self.ensure_accounts(concurrent_limit).await?;
            let num_accounts = self.accounts.lock().unwrap().len();
            let num_using_accounts = concurrent_limit.min(num_accounts);
            let intmax_senders = self.accounts.lock().unwrap()[0..num_using_accounts].to_vec();

            let mut futures = intmax_senders
                .iter()
                .enumerate()
                .map(|(i, sender)| {
                    let f = async move {
                        log::info!("Starting withdrawal from {} (No.{})", sender.pubkey, i);
                        let withdrawal_input = Transfer {
                            recipient: address_to_generic_address(trash_account.eth_address),
                            amount: U256::from(10),
                            token_index: ETH_TOKEN_INDEX,
                            salt: generate_salt(),
                        };
                        withdraw_directly_with_error_handling(*sender, withdrawal_input).await?;
                        log::info!("Withdrawal completed from {} (No.{})", sender.pubkey, i);

                        Ok::<(), Box<dyn std::error::Error>>(())
                    };
                    Box::pin(async move {
                        tokio::time::timeout(timeout_duration, f)
                            .await
                            .map_err(|_| format!("Operation {} timed out", i))?
                    })
                })
                .collect::<Vec<_>>();

            log::info!("Starting transactions");
            log_polling_futures(&mut futures, &intmax_senders).await;
        }

        Ok(())
    }
}
