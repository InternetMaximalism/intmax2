use std::{env::VarError, ops::Sub, str::FromStr, time::Duration};

use ethers::types::H256;
use fail::FailScenario;
use intmax2_cli::cli::{client::get_client, error::CliError};
use intmax2_client_sdk::{
    client::history::EntryStatus,
    external_api::{
        contract::utils::get_eth_balance,
        utils::retry::{retry_if, RetryConfig},
    },
};
use intmax2_interfaces::api::store_vault_server::types::{CursorOrder, MetaDataCursor};
use intmax2_zkp::{
    common::{deposit::Deposit, signature_content::key_set::KeySet},
    ethereum_types::{address::Address, bytes32::Bytes32, u256::U256, u32limb_trait::U32LimbTrait},
    utils::leafable::Leafable,
};
use rand::Rng;
use serde::Deserialize;
use tests::{
    accounts::{derive_custom_keys, private_key_to_account, Account},
    deposit_native_token_with_error_handling,
    ethereum::transfer_eth_on_ethereum,
    get_eth_balance_on_intmax,
    task::{process_queue, AsyncTask},
    wait_for_deposit_confirmation, wait_for_withdrawal_finalization,
    withdraw_directly_with_error_handling,
};
use tokio::time::sleep;

const ETH_TOKEN_INDEX: u32 = 0;
const RANDOM_ACTION_ACCOUNT_INDEX: u32 = 4;

#[derive(Debug, Deserialize)]
struct EnvVar {
    l1_rpc_url: String,
    deposit_admin_private_key: String,
    max_concurrent: Option<usize>,
    max_using_account: Option<usize>,
    account_offset: Option<usize>,
}

#[derive(Debug)]
struct TestSystem {
    l1_rpc_url: String,
    deposit_admin_key: Account,
}

// Failpoints that are safe to use with each action
const TRANSFER_SAFE_FAILPOINTS: &[&str] = &[
    "quote-transfer-fee-error",
    "quote-transfer-fee-beneficiary-missing",
    "quote-transfer-fee-collateral-without-fee",
    "send-tx-request-error",
    "send-tx-request-missing-fee-beneficiary",
    "send-tx-request-invalid-memo-index",
    "after-send-tx-request",
    "query-proposal-error",
    "query-proposal-limit-exceeded",
    "finalize-tx-error",
    "finalize-tx-zero-expiry",
    "finalize-tx-proposal-expired",
    "finalize-tx-expiry-too-far",
    "finalize-tx-transfer-data-not-found",
    "after-finalize-tx",
    "during-tx-status-polling",
    "tx-expired",
    "get-tx-status-error",
    "get_tx_status_returns_always_pending",
    "get_tx_status_returns_failed",
    "validity-witness-not-found",
    "sender-leaf-not-found",
    "sender-did-not-return-sig",
    "block-is-not-valid",
    "balance-insufficient-before-sync",
    "pending-tx-error",
];

impl TestSystem {
    fn new() -> Self {
        let config = envy::from_env::<EnvVar>().unwrap();
        Self {
            l1_rpc_url: config.l1_rpc_url,
            deposit_admin_key: private_key_to_account(
                H256::from_str(&config.deposit_admin_private_key).unwrap(),
            ),
        }
    }

    // Function to randomly enable a failpoint
    fn enable_random_failpoint() -> Option<FailScenario<'static>> {
        // 50% chance to enable a failpoint
        if cfg!(feature = "failpoints") && rand::thread_rng().gen_bool(0.5) {
            let failpoints = TRANSFER_SAFE_FAILPOINTS;

            let failpoint = failpoints[rand::thread_rng().gen_range(0..failpoints.len())];
            log::info!("Enabling failpoint: {}", failpoint);

            // Create and return the failpoint scenario
            let new_failpoints = format!("{}=return", failpoint);
            std::env::set_var("FAILPOINTS", new_failpoints);
            log::info!("scenario changed: {:?}", std::env::var("FAILPOINTS"));
            let scenario = FailScenario::setup();
            log::info!("Create scenario");
            Some(scenario)
        } else {
            None
        }
    }

    async fn execute_bridge_action(
        &self,
        keys: &[Account],
    ) -> Result<(), Box<dyn std::error::Error>> {
        // let switch_recipient = rand::thread_rng().gen_bool(0.5);
        // let sender_key = if switch_recipient { keys[1] } else { keys[0] };
        // let recipient_key = if switch_recipient { keys[0] } else { keys[1] };
        let sender_key = keys[0];
        let recipient_key = keys[1];
        let intermediate_key = keys[2];
        let sender_eth_balance = get_eth_balance(&self.l1_rpc_url, sender_key.eth_address).await?;
        let recipient_eth_balance =
            get_eth_balance(&self.l1_rpc_url, recipient_key.eth_address).await?;
        log::info!(
            "ETH balance of sender {}: {}",
            sender_key.eth_private_key,
            sender_eth_balance
        );
        log::info!(
            "ETH balance of recipient {} ({:?}): {} ",
            recipient_key.eth_private_key,
            recipient_key.eth_address,
            recipient_eth_balance
        );
        log::info!(
            "intermediate_key: {}, pubkey: {}",
            intermediate_key.intmax_key.privkey.to_hex(),
            intermediate_key.intmax_key.pubkey.to_hex(),
        );

        // Enable a random failpoint if applicable
        let scenario = Self::enable_random_failpoint();

        let result = self
            .execute_bridge(sender_key, intermediate_key.intmax_key, recipient_key)
            .await;

        // Clean up the failpoint if one was enabled
        if let Some(scenario) = scenario {
            scenario.teardown();
        }

        // Log the result
        match &result {
            Ok(_) => log::info!("Action completed successfully"),
            Err(e) => log::warn!(
                "Action failed with error {}: {:?}",
                sender_key.intmax_key.pubkey.to_hex(),
                e
            ),
        }

        // Return the result
        result
    }

    async fn execute_bridge(
        &self,
        sender_key: Account,
        intermediate_key: KeySet,
        recipient_key: Account,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.execute_deposit_wrapper(sender_key, intermediate_key)
            .await?;

        self.execute_withdrawal(intermediate_key, recipient_key, false)
            .await?;

        Ok(())
    }

    async fn execute_deposit_wrapper(
        &self,
        sender_key: Account,
        intermediate_key: KeySet,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let eth_balance = get_eth_balance_on_intmax(intermediate_key).await?;
        let amount = U256::from_hex("0x038d7ea4c68000").unwrap(); // 0.001 ETH
        log::info!(
            "ETH balance of intermediate {} ({}): {}",
            intermediate_key.privkey.to_hex(),
            intermediate_key.pubkey.to_hex(),
            eth_balance
        );

        if eth_balance.le(&amount) {
            let deposit_needed = self.get_deposit_needed(intermediate_key, amount).await?;
            if deposit_needed {
                log::info!(
                    "Deposit: {:?} -> {}",
                    sender_key.eth_address,
                    intermediate_key.pubkey.to_hex()
                );
                self.execute_deposit(sender_key, intermediate_key, amount)
                    .await?;

                println!("Sleeping for 10 minutes...");
                sleep(Duration::from_secs(600)).await;
            }
        }

        println!("Waiting for balance synchronization...");
        wait_for_deposit_confirmation(intermediate_key).await?;
        // wait_for_balance_synchronization(intermediate_key, Duration::from_secs(60)).await?;

        println!(
            "pubkey {} has sufficient balance {}",
            intermediate_key.pubkey.to_hex(),
            amount.to_string()
        );

        Ok(())
    }

    async fn get_deposit_needed(
        &self,
        intermediate_key: KeySet,
        amount: U256,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        let client = get_client().unwrap();
        let (deposit_history, _) = client
            .fetch_deposit_history(
                intermediate_key,
                &MetaDataCursor {
                    cursor: None,
                    order: CursorOrder::Desc,
                    limit: Some(1),
                },
            )
            .await?;

        match deposit_history.first() {
            Some(deposit) => {
                log::info!("Deposit history: {:?}", deposit);
                if deposit.status != EntryStatus::Pending {
                    log::warn!("(need deposit) No pending deposit was found");
                    return Ok(true);
                }

                let deposit_amount = deposit.data.amount;
                // deposit_amount < amount
                if !amount.le(&deposit_amount) {
                    log::warn!(
                        "(need deposit) Insufficient balance for withdrawal. Deposit amount: {}",
                        deposit_amount
                    );
                    return Ok(true);
                }

                // This address has enough balance
                log::info!(
                    "(not need deposit) Sufficient balance for withdrawal. Deposit amount: {}",
                    deposit_amount
                );
                return Ok(false);
            }
            None => {
                log::warn!("(need deposit) No deposit history found");
                return Ok(true);
            }
        };
    }

    async fn execute_deposit(
        &self,
        sender_key: Account,
        recipient_key: KeySet,
        deposit_amount: U256,
    ) -> Result<Bytes32, Box<dyn std::error::Error>> {
        let sender_initial_balance =
            get_eth_balance(&self.l1_rpc_url, sender_key.eth_address).await?;
        log::info!(
            "(deposit) initial balance of sender {}: {}",
            sender_key.eth_address,
            sender_initial_balance
        );
        let recipient_initial_balance = get_eth_balance_on_intmax(recipient_key).await?;
        log::info!(
            "(deposit) initial balance of recipient {}: {}",
            recipient_key.pubkey.to_hex(),
            recipient_initial_balance
        );
        println!("transfer amount: {}", deposit_amount.to_string());
        let fee = U256::from_str("0x038d7ea4c68000").unwrap(); // 0.001 ETH
        println!("fee: {}", fee.to_string());
        let transfer_amount_with_fee_hex = (deposit_amount + fee).to_hex();
        println!(
            "transfer_amount_with_fee_hex: {}",
            transfer_amount_with_fee_hex
        );
        let transfer_amount_with_fee =
            ethers::types::U256::from_str(&transfer_amount_with_fee_hex)?;
        println!(
            "transfer_amount_with_fee: {}",
            transfer_amount_with_fee.to_string()
        );

        // Abort test if balance is insufficient
        // sender_initial_balance < transfer_amount_with_fee
        if !transfer_amount_with_fee.le(&sender_initial_balance) {
            log::warn!(
                "Sender's balance is insufficient, so refill ETH from admin's address. Address: {:?}, Balance: {}",
                sender_key.eth_address,
                sender_initial_balance
            );

            let refilling_amount = transfer_amount_with_fee.sub(sender_initial_balance);
            log::info!("Refilling amount: {}", refilling_amount);
            self.refill_eth_on_ethereum(&[sender_key.eth_address], refilling_amount)
                .await?;
        }

        log::info!("Starting deposit {}", sender_key.eth_address);
        deposit_native_token_with_error_handling(
            sender_key.eth_private_key,
            recipient_key,
            deposit_amount,
            Some(Duration::from_secs(10)),
            false,
        )
        .await?;
        log::info!("Deposit completed {}", sender_key.eth_address);

        // Check final balances
        let sender_final_balance =
            get_eth_balance(&self.l1_rpc_url, sender_key.eth_address).await?;
        log::info!(
            "final balance of sender {}: {}",
            sender_key.eth_address,
            sender_final_balance
        );
        let recipient_final_balance = get_eth_balance_on_intmax(recipient_key).await?;
        log::info!(
            "final balance of recipient {}: {}",
            recipient_key.pubkey.to_hex(),
            recipient_final_balance
        );

        let deposit = Deposit {
            depositor: Address::from_bytes_be(sender_key.eth_address.as_bytes()).unwrap(),
            pubkey_salt_hash: Bytes32::rand(&mut rand::thread_rng()),
            amount: U256::from_str(&deposit_amount.to_string()).unwrap(),
            token_index: ETH_TOKEN_INDEX,
            is_eligible: true,
        };
        let deposit_hash = Leafable::hash(&deposit);

        Ok(deposit_hash)
    }

    async fn execute_withdrawal(
        &self,
        sender_key: KeySet,
        recipient_account: Account,
        with_claim_fee: bool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let sender_initial_balance = get_eth_balance_on_intmax(sender_key).await?;
        log::info!(
            "(withdrawal) initial balance of sender on Intmax {}: {}",
            sender_key.pubkey.to_hex(),
            sender_initial_balance
        );
        let recipient_initial_balance =
            get_eth_balance(&self.l1_rpc_url, recipient_account.eth_address).await?;
        log::info!(
            "(withdrawal) initial balance of recipient on Ethereum {}: {}",
            recipient_account.eth_address,
            recipient_initial_balance
        );

        let client = get_client().unwrap();
        let withdrawal_fee_result = client
            .withdrawal_server
            .get_withdrawal_fee()
            .await?
            .direct_withdrawal_fee
            .unwrap();
        let withdrawal_fee = withdrawal_fee_result
            .iter()
            .find(|fee| fee.token_index == ETH_TOKEN_INDEX)
            .unwrap();
        let claim_fee_result = client.withdrawal_server.get_claim_fee().await?.fee.unwrap();
        let claim_fee = claim_fee_result
            .iter()
            .find(|fee| fee.token_index == ETH_TOKEN_INDEX)
            .unwrap();
        log::info!("withdrawal_fee: {}", withdrawal_fee.amount.to_string());
        let mut transfer_amount = sender_initial_balance.sub(withdrawal_fee.amount);
        if with_claim_fee {
            log::info!("claim_fee: {}", claim_fee.amount.to_string());
            transfer_amount = transfer_amount.sub(claim_fee.amount);
        }

        // let block_builder_url =
        //     std::env::var("BLOCK_BUILDER_URL").expect("BLOCK_BUILDER_URL must be set");
        // let transfer_fee = client
        //     .block_builder
        //     .get_fee_info(&block_builder_url)
        //     .await?;
        let transfer_fee = U256::from_str("67000000000000").unwrap(); // TODO: valid value
        log::info!("transfer_fee: {}", transfer_fee);
        transfer_amount = transfer_amount.sub(transfer_fee); // TODO: valid value

        log::info!("max_withdrawal_amount: {}", transfer_amount);

        let to = Address::from_bytes_be(recipient_account.eth_address.as_bytes()).unwrap();
        log::info!("withdrawal recipient: {}", to);

        log::info!("Starting withdrawal {}", sender_key.pubkey);
        withdraw_directly_with_error_handling(
            sender_key,
            to,
            transfer_amount,
            ETH_TOKEN_INDEX,
            true,
        )
        .await?;
        log::info!("Withdrawal completed {}", sender_key.pubkey);

        let retry_config = RetryConfig {
            max_retries: 100,
            initial_delay: 30000,
        };
        let retry_condition = |_: &CliError| true;
        retry_if(
            retry_condition,
            || wait_for_withdrawal_finalization(sender_key),
            retry_config,
        )
        .await?;

        // Check final balances
        let sender_final_balance = get_eth_balance_on_intmax(sender_key).await?;
        log::info!(
            "final balance of sender {}: {}",
            sender_key.pubkey.to_hex(),
            sender_final_balance
        );
        let recipient_final_balance =
            get_eth_balance(&self.l1_rpc_url, recipient_account.eth_address).await?;
        log::info!(
            "final balance of recipient {}: {}",
            recipient_account.eth_address,
            recipient_final_balance
        );

        Ok::<(), Box<dyn std::error::Error>>(())
    }

    async fn refill_eth_on_ethereum(
        &self,
        intmax_recipients: &[ethers::types::Address],
        amount: ethers::types::U256,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let balance = get_eth_balance(&self.l1_rpc_url, self.deposit_admin_key.eth_address).await?;
        log::info!("Admin's balance: {}", balance);
        log::info!("Admin's address: {:?}", self.deposit_admin_key.eth_address);
        let private_key = self.deposit_admin_key.eth_private_key.to_hex();
        for recipient in intmax_recipients {
            transfer_eth_on_ethereum(&self.l1_rpc_url, &private_key, *recipient, amount).await?;
        }

        Ok(())
    }
}

#[derive(Debug)]
struct RandomActionTask;

#[async_trait::async_trait(?Send)]
impl AsyncTask for RandomActionTask {
    type Output = ();
    type Error = VarError;

    async fn execute(id: usize, account_index: u32) -> Result<Self::Output, Self::Error> {
        log::info!("Starting random action test (id: {})", id);

        let master_mnemonic =
            std::env::var("MASTER_MNEMONIC").expect("MASTER_MNEMONIC must be set");
        let sender_keys = derive_custom_keys(
            &master_mnemonic,
            account_index,
            RANDOM_ACTION_ACCOUNT_INDEX,
            // 3,
            (id * 3) as u32,
        )
        .unwrap();

        let test_system = TestSystem::new();
        let result = test_system
            .execute_bridge_action(sender_keys.as_slice())
            .await;
        match result {
            Ok(_) => {
                log::info!("(id: {}) test completed", id);
            }
            Err(e) => {
                log::warn!("(id: {}) failed: {:?}", id, e);
            }
        }

        Ok(())
    }
}

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

    // Run more iterations to test more failpoints
    process_queue::<RandomActionTask>(
        config.max_using_account.unwrap_or(3),
        config.max_concurrent.unwrap_or(1),
        config.account_offset.unwrap_or(0),
    )
    .await;

    Ok(())
}
