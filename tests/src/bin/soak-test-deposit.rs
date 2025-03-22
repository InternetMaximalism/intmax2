use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use ethers::{
    types::H256,
    utils::{hex, parse_ether},
};
use futures::future::join_all;
use intmax2_cli::cli::error::CliError;
use intmax2_client_sdk::{
    client::key_from_eth::generate_intmax_account_from_eth_key,
    external_api::utils::query::get_request,
};
use intmax2_zkp::ethereum_types::{u256::U256, u32limb_trait::U32LimbTrait};
use serde::Deserialize;
use tests::{
    accounts::{derive_deposit_keys, mnemonic_to_account, private_key_to_account, Account},
    deposit_native_token_with_error_handling,
    ethereum::{get_balance, transfer_eth_batch_on_ethereum, transfer_eth_on_ethereum},
};

#[derive(Debug, Clone, Deserialize)]
pub struct EnvVar {
    pub master_mnemonic: String,
    pub deposit_admin_private_key: String, // Ethereum private key
    pub num_of_recipients: Option<u32>,
    pub recipient_offset: Option<u32>,
    pub balance_prover_base_url: String,
    pub l1_rpc_url: String,
    pub l1_chain_id: u64,

    #[serde(default = "default_url")]
    pub config_server_base_url: String,
}

fn default_url() -> String {
    "0.0.0.0:8080".to_string()
}

const DEPOSIT_TIMEOUT: u64 = 5 * 60;

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

    log::info!("Start deposit soak test");
    let private_key = H256::from_slice(&hex::decode(config.deposit_admin_private_key.clone())?);
    let admin_intmax_key = generate_intmax_account_from_eth_key(private_key);
    log::info!("Admin pubkey: {}", admin_intmax_key.pubkey.to_hex());

    let config_server_base_url = config.config_server_base_url.to_string();

    let system = TestSystem::new(
        config.deposit_admin_private_key,
        config.master_mnemonic,
        config_server_base_url,
        config.l1_rpc_url,
    );
    system.run_soak_test().await?;
    log::info!("End deposit soak test");

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
    admin_eth_key: String,
    master_mnemonic_phrase: String,
    config_server_base_url: String,
    l1_rpc_url: String,
    accounts: Arc<Mutex<Vec<Account>>>,
}

impl TestSystem {
    pub fn new(
        admin_eth_key: String,
        master_mnemonic_phrase: String,
        config_server_base_url: String,
        l1_rpc_url: String,
    ) -> Self {
        let trash_account = mnemonic_to_account(&master_mnemonic_phrase, 1, 0).unwrap();
        println!(
            "Trash account intmax_private_key: {}",
            trash_account.intmax_key.privkey
        );
        println!(
            "Trash account eth_private_key: {:?}",
            trash_account.eth_private_key
        );
        Self {
            admin_eth_key,
            master_mnemonic_phrase,
            accounts: Arc::new(Mutex::new(Vec::new())),
            l1_rpc_url: l1_rpc_url.to_string(),
            config_server_base_url,
        }
    }

    async fn ensure_accounts(
        &self,
        required_count: usize,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let accounts_len = self.accounts.lock().unwrap().len();

        let num_of_keys = if accounts_len >= required_count {
            0
        } else {
            required_count - accounts_len
        };

        // Create new account and receive initial balance from admin
        let master_mnemonic_phrase = self.master_mnemonic_phrase.clone();
        let new_accounts = derive_deposit_keys(
            &master_mnemonic_phrase,
            num_of_keys as u32,
            accounts_len as u32,
        )?;
        self.accounts.lock().unwrap().extend(new_accounts.iter());

        let accounts = self.accounts.lock().unwrap().clone();
        let amount = parse_ether(0.005)?;
        let result = self.distribute(&accounts, amount, 1).await;
        match result {
            Ok(_) => log::info!("Distributed balance successfully"),
            Err(e) => log::error!("Failed to distribute balance: {:?}", e),
        }

        Ok(())
    }

    /// Distribute the given amount to the accounts.
    async fn distribute(
        &self,
        accounts: &[Account],
        amount: ethers::types::U256,
        max_transfers_per_transaction: usize,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let min_gas_price = parse_ether(0.002)?;
        let admin_eth_key = H256::from_slice(&hex::decode(&self.admin_eth_key)?);
        let admin_account = private_key_to_account(admin_eth_key);
        log::info!("admin Ethereum address: {}", admin_account.eth_address);
        for account_subset in accounts.chunks(max_transfers_per_transaction) {
            if max_transfers_per_transaction == 1 {
                let recipient = account_subset[0].eth_address;
                let balance = get_balance(&self.l1_rpc_url, recipient).await?;
                if balance > min_gas_price {
                    log::info!(
                        "Skipping transfer to {} as it already has enough balance",
                        recipient
                    );
                    continue;
                }
                transfer_eth_on_ethereum(
                    &self.l1_rpc_url,
                    &self.admin_eth_key,
                    recipient,
                    amount - balance,
                )
                .await?;
            } else {
                transfer_eth_batch_on_ethereum(
                    &self.l1_rpc_url,
                    &self.admin_eth_key,
                    &account_subset
                        .iter()
                        .map(|account| account.eth_address)
                        .collect::<Vec<_>>(),
                    amount,
                )
                .await?;
            }
        }

        Ok(())
    }

    pub async fn run_soak_test(&self) -> Result<(), Box<dyn std::error::Error>> {
        let trash_account = mnemonic_to_account(&self.master_mnemonic_phrase, 1, 0)?;

        // self.ensure_accounts_without_transfers(400).await?;
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

            let futures = intmax_senders
                .iter()
                .enumerate()
                .map(|(i, sender)| async move {
                    log::info!("Starting deposit from {} (No.{})", sender.eth_address, i);
                    deposit_native_token_with_error_handling(
                        sender.eth_private_key,
                        trash_account.intmax_key,
                        U256::from(10),
                        None,
                        false,
                    )
                    .await?;
                    log::info!("Deposit completed from {:?} (No.{})", sender.eth_address, i);

                    Ok::<(), CliError>(())
                });

            log::info!("Starting transactions");
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(DEPOSIT_TIMEOUT)) => log::info!("transaction timeout"),
                errors = join_all(futures) => {
                    for (i, error) in errors.iter().enumerate() {
                        if let Err(e) = error {
                            log::error!("Recipient ({}/{}) failed: {:?}", i + 1, num_using_accounts, e);
                        } else {
                            log::info!("Recipient ({}/{}) succeeded", i + 1, num_using_accounts);
                        }
                    }

                    log::info!("Completed transactions");
                },
            }
        }

        Ok(())
    }
}
