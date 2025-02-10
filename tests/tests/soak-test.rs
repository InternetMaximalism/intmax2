use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use ethers::{types::H256, utils::hex};
use intmax2_cli::{
    cli::{
        error::CliError,
        send::{transfer, TransferInput},
    },
    format::privkey_to_keyset,
};
use intmax2_zkp::{
    common::signature::key_set::KeySet, ethereum_types::u32limb_trait::U32LimbTrait,
};
use serde::Deserialize;
use tests::{derive_intmax_keys, wait_for_balance_synchronization};

#[derive(Debug, Clone, Deserialize)]
pub struct EnvVar {
    pub master_mnemonic: String,
    pub private_key: String,
    pub num_of_recipients: Option<u32>,
    pub recipient_offset: Option<u32>,
    pub balance_prover_base_url: String,
    pub concurrent_limit: Option<usize>,
    pub cool_down_seconds: Option<u64>,
}

const ETH_TOKEN_INDEX: u32 = 0;

// async fn process_account(key: KeySet, transfers: &[TransferInput]) -> Result<(), CliError> {
async fn process_account(
    key: KeySet,
    transfers: &[TransferInput],
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    transfer(key, transfers, ETH_TOKEN_INDEX)
        .await
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    tokio::time::sleep(Duration::from_secs(20)).await;
    wait_for_balance_synchronization(key, Duration::from_secs(5))
        .await
        .map_err(|e| e.into())
}

#[tokio::test]
async fn test_soak_block_generation() -> Result<(), Box<dyn std::error::Error>> {
    dotenv::dotenv()?;
    dotenv::from_path("../cli/.env")?;
    let config = envy::from_env::<EnvVar>().unwrap();
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .init();

    let private_key = H256::from_slice(&hex::decode(config.private_key)?);
    let admin_key = privkey_to_keyset(private_key);

    let concurrent_limit = 5;
    let duration_secs = 60;
    let system = TestSystem::new(admin_key, config.master_mnemonic);
    system
        .run_soak_test(concurrent_limit, duration_secs)
        .await?;

    Ok(())
}

#[derive(Debug, Clone, Deserialize)]
struct TestConfig {
    concurrent_limit: usize,
}

async fn get_config() -> Result<TestConfig, Box<dyn std::error::Error>> {
    let config = envy::from_env::<TestConfig>()?;
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

    async fn transfer_from_admin(&self, intmax_recipients: &[KeySet]) -> Result<(), CliError> {
        let transfers = intmax_recipients
            .iter()
            .map(|recipient| TransferInput {
                recipient: recipient.pubkey.to_hex(),
                amount: 1000000000u128,
                token_index: ETH_TOKEN_INDEX,
            })
            .collect::<Vec<_>>();
        transfer(self.admin_key, &transfers, ETH_TOKEN_INDEX).await
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

        let chunk_size = 63;
        for chunk in new_accounts.chunks(chunk_size) {
            self.transfer_from_admin(chunk).await?;
            self.accounts.lock().unwrap().extend(chunk.iter());
        }

        Ok(())
    }

    pub async fn run_soak_test(
        &self,
        concurrent_accounts: usize,
        duration_secs: u64,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Ensure we have enough accounts
        self.ensure_accounts(concurrent_accounts).await?;

        let end_time = std::time::Instant::now() + Duration::from_secs(duration_secs);

        while std::time::Instant::now() < end_time {
            let concurrent_accounts = get_config().await?.concurrent_limit;

            let mut handles = vec![];

            // Spawn concurrent transactions
            for i in 0..concurrent_accounts {
                let system = Arc::new(self.clone());
                let account = system.accounts.lock().unwrap()[i];
                let handle = tokio::task::spawn_local(async move {
                    let transfers = [TransferInput {
                        recipient: system.admin_key.pubkey.to_hex(),
                        amount: 10u128,
                        token_index: ETH_TOKEN_INDEX,
                    }];
                    if let Err(e) = process_account(account, &transfers).await {
                        eprintln!("Transaction error: {}", e);
                    }
                });
                handles.push(handle);
            }

            // Wait for all transactions to complete
            for handle in handles {
                handle.await?;
            }
        }

        Ok(())
    }
}
