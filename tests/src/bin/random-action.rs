use std::{
    env::VarError,
    ops::{Add, Sub},
    str::FromStr,
    time::Duration,
};

use ethers::{abi::AbiEncode, types::H256};
use fail::FailScenario;
use intmax2_cli::cli::{error::CliError, send::send_transfers};
use intmax2_client_sdk::{
    client::sync::utils::generate_salt,
    external_api::{
        contract::utils::get_eth_balance,
        utils::retry::{retry_if, RetryConfig},
    },
};
use intmax2_zkp::{
    common::{generic_address::GenericAddress, signature::key_set::KeySet, transfer::Transfer},
    ethereum_types::{address::Address, u256::U256, u32limb_trait::U32LimbTrait},
};
use rand::Rng;
use serde::{Deserialize, Serialize};
use tests::{
    accounts::{derive_custom_keys, private_key_to_account, Account},
    deposit_native_token_with_error_handling,
    ethereum::transfer_eth_on_ethereum,
    get_eth_balance_on_intmax,
    task::{process_queue, AsyncTask},
    wait_for_balance_synchronization, wait_for_withdrawal_finalization,
    withdraw_directly_with_error_handling,
};

const ETH_TOKEN_INDEX: u32 = 0;
const RANDOM_ACTION_ACCOUNT_INDEX: u32 = 4;

#[derive(Debug, Deserialize)]
struct EnvVar {
    l1_rpc_url: String,
    transfer_admin_private_key: String,
    max_concurrent: Option<usize>,
    max_using_account: Option<usize>,
    account_index: Option<u32>,
    account_offset: Option<usize>,
    action: Option<Action>,
}

#[derive(Debug)]
struct TestSystem {
    l1_rpc_url: String,
    admin_key: Account,
    action: Option<Action>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
enum Action {
    #[serde(rename = "deposit")]
    Deposit,
    #[serde(rename = "transfer")]
    Transfer,
    #[serde(rename = "withdrawal")]
    Withdrawal,
}

impl Action {
    fn random() -> Self {
        let actions = [
            Action::Deposit,
            Action::Transfer,
            Action::Transfer,
            Action::Transfer,
            Action::Withdrawal,
            Action::Withdrawal,
        ];
        actions[rand::thread_rng().gen_range(0..actions.len())]
    }
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
    "after-update-pending-withdrawals",
];

const WITHDRAWAL_SAFE_FAILPOINTS: &[&str] = &[
    "after-update-pending-withdrawals",
    "after-sync-withdrawals",
    "before-request-withdrawal",
    "after-request-withdrawal",
    "after-consume_payment",
    "save-user-data",
];

impl TestSystem {
    fn new() -> Self {
        let config = envy::from_env::<EnvVar>().unwrap();
        Self {
            l1_rpc_url: config.l1_rpc_url,
            admin_key: private_key_to_account(
                H256::from_str(&config.transfer_admin_private_key).unwrap(),
            ),
            action: config.action,
        }
    }

    // Function to randomly enable a failpoint
    fn enable_random_failpoint(action: Action) -> Option<FailScenario<'static>> {
        // 50% chance to enable a failpoint
        if cfg!(feature = "failpoints") && rand::thread_rng().gen_bool(0.5) {
            let failpoints = match action {
                Action::Transfer => TRANSFER_SAFE_FAILPOINTS,
                Action::Withdrawal => {
                    &[TRANSFER_SAFE_FAILPOINTS, WITHDRAWAL_SAFE_FAILPOINTS].concat()
                }
                // For now, only use failpoints with Transfer action
                _ => return None,
            };
            if failpoints.is_empty() {
                return None;
            }

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

    async fn execute_random_action(
        &self,
        keys: &[Account],
    ) -> Result<(), Box<dyn std::error::Error>> {
        let action = self.action.unwrap_or_else(Action::random);
        let switch_recipient = rand::thread_rng().gen_bool(0.5);
        let sender_key = if switch_recipient { keys[1] } else { keys[0] };
        let recipient_key = if switch_recipient { keys[0] } else { keys[1] };
        log::info!("sender_key: {}", sender_key.intmax_key.privkey);
        log::info!("recipient_key: {}", recipient_key.intmax_key.privkey);

        // Enable a random failpoint if applicable
        let scenario = Self::enable_random_failpoint(action);

        let result = match action {
            Action::Deposit => {
                log::info!(
                    "Deposit: {:?} -> {}",
                    sender_key.eth_address,
                    recipient_key.intmax_key.pubkey.to_hex()
                );
                self.execute_deposit(sender_key, recipient_key.intmax_key)
                    .await
            }
            Action::Transfer => {
                log::info!(
                    "Transfer: {} -> {}",
                    sender_key.intmax_key.pubkey.to_hex(),
                    recipient_key.intmax_key.pubkey.to_hex()
                );
                self.execute_transfer(sender_key.intmax_key, recipient_key.intmax_key)
                    .await
            }
            Action::Withdrawal => {
                log::info!(
                    "Withdrawal: {} -> {:?}",
                    sender_key.intmax_key.pubkey.to_hex(),
                    recipient_key.eth_address
                );
                self.execute_withdrawal(sender_key.intmax_key, recipient_key)
                    .await
            }
        };

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

    async fn execute_deposit(
        &self,
        sender_key: Account,
        recipient_key: KeySet,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let sender_initial_balance =
            get_eth_balance(&self.l1_rpc_url, sender_key.eth_address).await?;
        log::info!(
            "initial balance of sender {}: {}",
            sender_key.eth_address,
            sender_initial_balance
        );
        let recipient_initial_balance = get_eth_balance_on_intmax(recipient_key).await?;
        log::info!(
            "initial balance of recipient {}: {}",
            recipient_key.pubkey.to_hex(),
            recipient_initial_balance
        );
        let transfer_amount = ethers::types::U256::from(10000);
        let fee = ethers::types::U256::from_str("0x38d7ea4c68000").unwrap();
        let transfer_amount_with_fee = transfer_amount.add(fee);

        // Abort test if balance is insufficient
        if sender_initial_balance.lt(&transfer_amount_with_fee) {
            log::warn!(
                "Sender's balance is insufficient. Address: {}, Balance: {}",
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
            U256::from(10),
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
        let expected_recipient_balance =
            recipient_initial_balance + U256::from_hex(&transfer_amount.encode_hex()).unwrap();

        if recipient_final_balance == expected_recipient_balance {
            log::info!("transfer transaction was processed");
        } else {
            log::warn!("Recipient's balance does not match expected value");
            log::warn!(
                "Expected: {}, Actual: {}",
                expected_recipient_balance,
                recipient_final_balance
            );
        }

        Ok(())
    }

    async fn execute_transfer(
        &self,
        sender_key: KeySet,
        recipient_key: KeySet,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let sender_initial_balance = get_eth_balance_on_intmax(sender_key).await?;
        log::info!(
            "initial balance of sender {}: {}",
            sender_key.pubkey.to_hex(),
            sender_initial_balance
        );
        let recipient_initial_balance = get_eth_balance_on_intmax(recipient_key).await?;
        log::info!(
            "initial balance of recipient {}: {}",
            recipient_key.pubkey.to_hex(),
            recipient_initial_balance
        );

        let transfer_amount = U256::from(17);
        let fee = U256::from_hex("0x246139ca800").unwrap(); // 2500000000000
        let transfer_amount_with_fee = transfer_amount.add(fee);

        // Abort test if balance is insufficient
        if sender_initial_balance.lt(&transfer_amount_with_fee) {
            log::warn!(
                "Sender's balance is insufficient. Address: {}, Balance: {}",
                sender_key.pubkey.to_hex(),
                sender_initial_balance
            );

            let refilling_amount = transfer_amount_with_fee.sub(sender_initial_balance);
            log::info!("Refilling amount: {}", refilling_amount);
            self.refill_eth_on_intmax(&[sender_key], refilling_amount)
                .await?;
        }

        let transfer = Transfer {
            recipient: GenericAddress::from_pubkey(recipient_key.pubkey),
            amount: transfer_amount,
            token_index: ETH_TOKEN_INDEX,
            salt: generate_salt(),
        };

        let result = send_transfers(sender_key, &[transfer], vec![], ETH_TOKEN_INDEX, true).await;
        match &result {
            Ok(_) => log::info!("Transaction completed successfully"),
            Err(e) => log::warn!(
                "Transaction failed with error {}: {:?}",
                sender_key.pubkey.to_hex(),
                e
            ),
        }

        // Check final balances
        let sender_final_balance = get_eth_balance_on_intmax(sender_key).await?;
        log::info!(
            "final balance of sender {}: {}",
            sender_key.pubkey.to_hex(),
            sender_final_balance
        );
        let recipient_final_balance = get_eth_balance_on_intmax(recipient_key).await?;
        log::info!(
            "final balance of recipient {}: {}",
            recipient_key.pubkey.to_hex(),
            recipient_final_balance
        );

        // Only check balance if the transaction succeeded
        if result.is_ok() {
            // Expected result: Recipient's balance should increase by only one transfer amount
            let expected_recipient_balance = recipient_initial_balance + transfer_amount;

            if recipient_final_balance == expected_recipient_balance {
                log::info!("Only one of the two transfer transactions was processed");
            } else {
                log::warn!("Recipient's balance does not match expected value");
                log::warn!(
                    "Expected: {}, Actual: {}",
                    expected_recipient_balance,
                    recipient_final_balance
                );
            }
        }

        // Return the result of send_transfers
        result.map_err(|e| e.into())
    }

    async fn execute_withdrawal(
        &self,
        sender_key: KeySet,
        recipient_account: Account,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let sender_initial_balance = get_eth_balance_on_intmax(sender_key).await?;
        log::info!(
            "initial balance of sender {}: {}",
            sender_key.pubkey.to_hex(),
            sender_initial_balance
        );
        let recipient_initial_balance =
            get_eth_balance(&self.l1_rpc_url, recipient_account.eth_address).await?;
        log::info!(
            "initial balance of recipient {}: {}",
            recipient_account.eth_address,
            recipient_initial_balance
        );

        let transfer_amount = U256::from(19);
        let fee = U256::from_hex("0x1fd512913000").unwrap(); // 2500000000000 + 32500000000000
        let transfer_amount_with_fee = transfer_amount.add(fee);

        // Abort test if balance is insufficient
        if sender_initial_balance.lt(&transfer_amount_with_fee) {
            log::warn!(
                "Sender's balance is insufficient. Pubkey: {}, Balance: {}",
                sender_key.pubkey.to_hex(),
                sender_initial_balance
            );

            let refilling_amount = transfer_amount_with_fee.sub(sender_initial_balance);
            log::info!("Refilling amount: {}", refilling_amount);
            self.refill_eth_on_intmax(&[sender_key], refilling_amount)
                .await?;
        }

        let to = Address::from_bytes_be(recipient_account.eth_address.as_bytes()).unwrap();
        log::info!("withdrawal recipient: {}", to);

        log::info!("Starting withdrawal {}", sender_key.pubkey);
        withdraw_directly_with_error_handling(
            sender_key,
            to,
            transfer_amount,
            ETH_TOKEN_INDEX,
            false,
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

        // let expected_recipient_balance = U256::from(recipient_initial_balance) + (transfer_amount);

        // if recipient_final_balance.eq(&expected_recipient_balance) {
        //     log::info!("Withdrawal transaction was processed");
        // } else {
        //     log::warn!("Recipient's balance does not match expected value");
        //     log::warn!(
        //         "Expected: {}, Actual: {}",
        //         expected_recipient_balance,
        //         recipient_final_balance
        //     );
        // }

        Ok::<(), Box<dyn std::error::Error>>(())
    }

    async fn refill_eth_on_intmax(
        &self,
        intmax_recipients: &[KeySet],
        amount: U256,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let balance = get_eth_balance_on_intmax(self.admin_key.intmax_key).await?;
        log::info!("Admin's balance: {}", balance);

        let chunk_size = 63;
        for recipients in intmax_recipients.chunks(chunk_size) {
            transfer_from(self.admin_key.intmax_key, recipients, amount).await?;
        }

        Ok(())
    }

    async fn refill_eth_on_ethereum(
        &self,
        intmax_recipients: &[ethers::types::Address],
        amount: ethers::types::U256,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let balance = get_eth_balance(&self.l1_rpc_url, self.admin_key.eth_address).await?;
        log::info!("Admin's balance: {}", balance);
        log::info!("Admin's address: {:?}", self.admin_key.eth_address);
        for recipient in intmax_recipients {
            transfer_eth_on_ethereum(
                &self.l1_rpc_url,
                &format!("{:064x}", self.admin_key.eth_private_key),
                *recipient,
                amount,
            )
            .await?;
        }

        Ok(())
    }
}

async fn transfer_from(
    sender: KeySet,
    intmax_recipients: &[KeySet],
    amount: U256,
) -> Result<(), CliError> {
    wait_for_balance_synchronization(sender, Duration::from_secs(5)).await?;
    let transfers = intmax_recipients
        .iter()
        .map(|recipient| Transfer {
            recipient: GenericAddress::from_pubkey(recipient.pubkey),
            amount,
            token_index: ETH_TOKEN_INDEX,
            salt: generate_salt(),
        })
        .collect::<Vec<_>>();
    log::info!("Transfers: {:?}", transfers);
    send_transfers(sender, &transfers, vec![], ETH_TOKEN_INDEX, true).await
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
        let sender_keys =
            derive_custom_keys(&master_mnemonic, account_index, 2, (id * 2) as u32).unwrap();

        let test_system = TestSystem::new();
        let result = test_system
            .execute_random_action(sender_keys.as_slice())
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
