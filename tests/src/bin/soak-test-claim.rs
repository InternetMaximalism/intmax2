use std::{
    env::VarError,
    ops::{Add, Sub},
    str::FromStr,
    time::Duration,
};

use ethers::{abi::AbiEncode, types::H256};
use fail::FailScenario;
use intmax2_cli::cli::{claim, client::get_client, error::CliError, send::send_transfers};
use intmax2_client_sdk::{
    client::{
        history::{EntryStatus, HistoryEntry},
        strategy::mining::MiningStatus,
        sync::utils::generate_salt,
    },
    external_api::{
        contract::utils::get_eth_balance,
        utils::retry::{retry_if, RetryConfig},
    },
};
use intmax2_interfaces::{
    api::store_vault_server::types::{CursorOrder, MetaDataCursor},
    data::{deposit_data::DepositData, meta_data::MetaData},
};
use intmax2_zkp::{
    common::{
        deposit::Deposit, generic_address::GenericAddress, signature::key_set::KeySet,
        transfer::Transfer,
    },
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
    account_offset: Option<usize>,
}

#[derive(Debug)]
struct TestSystem {
    l1_rpc_url: String,
    admin_key: Account,
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
            admin_key: private_key_to_account(
                H256::from_str(&config.transfer_admin_private_key).unwrap(),
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

    async fn execute_random_action(
        &self,
        keys: &[Account],
    ) -> Result<(), Box<dyn std::error::Error>> {
        let switch_recipient = rand::thread_rng().gen_bool(0.5);
        let sender_key = if switch_recipient { keys[1] } else { keys[0] };
        let recipient_key = if switch_recipient { keys[0] } else { keys[1] };
        let intermediate_key = keys[2];
        log::info!("sender_key: {}", sender_key.intmax_key.privkey);
        log::info!("recipient_key: {}", recipient_key.intmax_key.privkey);

        // Enable a random failpoint if applicable
        let scenario = Self::enable_random_failpoint();

        let result = self
            .execute_mining(sender_key, intermediate_key.intmax_key, recipient_key)
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

    async fn execute_mining(
        &self,
        sender_key: Account,
        intermediate_key: KeySet,
        recipient_key: Account,
    ) -> Result<(), Box<dyn std::error::Error>> {
        log::info!(
            "Deposit: {:?} -> {}",
            sender_key.eth_address,
            intermediate_key.pubkey.to_hex()
        );
        let deposit_hash = self.execute_deposit(sender_key, intermediate_key).await?;
        self.wait_for_mining_claimable(intermediate_key, deposit_hash)
            .await?;
        log::info!(
            "Withdrawal: {} -> {:?}",
            intermediate_key.pubkey.to_hex(),
            recipient_key.eth_address
        );
        self.execute_withdrawal(intermediate_key, recipient_key)
            .await?;

        Ok(())
    }

    async fn execute_deposit(
        &self,
        sender_key: Account,
        recipient_key: KeySet,
    ) -> Result<Bytes32, Box<dyn std::error::Error>> {
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
        println!("0x{:0>64}", "16345785d8a0000");
        let deposit_amount = U256::from_hex(&format!("0x{:0>64}", "16345785d8a0000")).unwrap(); // 0.1 ETH
        let transfer_amount = ethers::types::U256::from_str("16345785d8a0000").unwrap(); // 0.1 ETH
        let fee = ethers::types::U256::from_str("0x38d7ea4c68000").unwrap(); // 0.001 ETH
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
            deposit_amount,
            Some(Duration::from_secs(10)),
            true,
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
        // let expected_recipient_balance =
        //     recipient_initial_balance + U256::from_hex(&transfer_amount.encode_hex()).unwrap();

        // if recipient_final_balance == expected_recipient_balance {
        //     log::info!("transfer transaction was processed");
        // } else {
        //     log::warn!("Recipient's balance does not match expected value");
        //     log::warn!(
        //         "Expected: {}, Actual: {}",
        //         expected_recipient_balance,
        //         recipient_final_balance
        //     );
        // }

        let deposit = Deposit {
            depositor: Address::from_bytes_be(sender_key.eth_address.as_bytes()),
            pubkey_salt_hash: Bytes32::rand(&mut rand::thread_rng()),
            amount: U256::from_str(&deposit_amount.to_string()).unwrap(),
            token_index: ETH_TOKEN_INDEX,
            is_eligible: true,
        };
        let deposit_hash = Leafable::hash(&deposit);

        Ok(deposit_hash)
    }

    async fn wait_for_mining_claimable(
        &self,
        key: KeySet,
        target_deposit_hash: Bytes32,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let client = get_client().unwrap();

        let retry_config = RetryConfig {
            max_retries: 100,
            initial_delay: 30000,
        };
        let retry_condition = |_: &CliError| true;

        // let order = CursorOrder::Desc;
        // let from_timestamp = None;
        // let cursor = MetaDataCursor {
        //     cursor: from_timestamp.map(|timestamp| MetaData {
        //         timestamp,
        //         digest: Bytes32::default(),
        //     }),
        //     order: order.clone(),
        //     limit: None,
        // };
        // let (deposit_history, _) = client.fetch_deposit_history(key, &cursor).await?;
        // let latest_deposit = deposit_history.first().unwrap();
        // wait_for_deposit_confirmation(key, latest_deposit).await?;

        loop {
            let mining_list = retry_if(
                retry_condition,
                || async { client.get_mining_list(key).await.map_err(|e| e.into()) },
                retry_config,
            )
            .await?;

            let target_mining = mining_list
                .into_iter()
                .find(|mining| mining.deposit_data.deposit_hash() == Some(target_deposit_hash));

            if let Some(target_mining) = target_mining {
                if matches!(target_mining.status, MiningStatus::Claimable(_)) {
                    return Ok(());
                }

                log::warn!("Mining is not claimable yet. Retrying in 60 seconds");
                tokio::time::sleep(Duration::from_secs(60)).await;
                continue;
            }

            log::warn!("Mining is not found yet. Retrying in 60 seconds");
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
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

        let client = get_client().unwrap();
        let withdrawal_fee_result = client
            .withdrawal_server
            .get_withdrawal_fee()
            .await?
            .direct_withdrawal_fee
            .unwrap();
        let claim_fee_result = client.withdrawal_server.get_claim_fee().await?.fee.unwrap();
        let withdrawal_fee = withdrawal_fee_result
            .iter()
            .find(|fee| fee.token_index == ETH_TOKEN_INDEX)
            .unwrap();
        let claim_fee = claim_fee_result
            .iter()
            .find(|fee| fee.token_index == ETH_TOKEN_INDEX)
            .unwrap();
        let transfer_amount = sender_initial_balance
            .sub(withdrawal_fee.amount)
            .sub(claim_fee.amount);
        log::info!("max_withdrawal_amount: {}", transfer_amount);

        // // Abort test if balance is insufficient
        // if sender_initial_balance.lt(&transfer_amount_with_fee) {
        //     log::warn!(
        //         "Sender's balance is insufficient. Pubkey: {}, Balance: {}",
        //         sender_key.pubkey.to_hex(),
        //         sender_initial_balance
        //     );

        //     let refilling_amount = transfer_amount_with_fee.sub(sender_initial_balance);
        //     log::info!("Refilling amount: {}", refilling_amount);
        //     self.refill_eth_on_intmax(&[sender_key], refilling_amount)
        //         .await?;
        // }

        let to = Address::from_bytes_be(recipient_account.eth_address.as_bytes());
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

    // async fn refill_eth_on_intmax(
    //     &self,
    //     intmax_recipients: &[KeySet],
    //     amount: U256,
    // ) -> Result<(), Box<dyn std::error::Error>> {
    //     let balance = get_eth_balance_on_intmax(self.admin_key.intmax_key).await?;
    //     log::info!("Admin's balance: {}", balance);

    //     let chunk_size = 63;
    //     for recipients in intmax_recipients.chunks(chunk_size) {
    //         transfer_from(self.admin_key.intmax_key, recipients, amount).await?;
    //     }

    //     Ok(())
    // }

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

async fn wait_for_deposit_confirmation(
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

// async fn transfer_from([2]
//     sender: KeySet,
//     intmax_recipients: &[KeySet],
//     amount: U256,
// ) -> Result<(), CliError> {
//     wait_for_balance_synchronization(sender, Duration::from_secs(5)).await?;
//     let transfers = intmax_recipients
//         .iter()
//         .map(|recipient| Transfer {
//             recipient: GenericAddress::from_pubkey(recipient.pubkey),
//             amount,
//             token_index: ETH_TOKEN_INDEX,
//             salt: generate_salt(),
//         })
//         .collect::<Vec<_>>();
//     log::info!("Transfers: {:?}", transfers);
//     send_transfers(sender, &transfers, vec![], ETH_TOKEN_INDEX, true).await
// }

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
            RANDOM_ACTION_ACCOUNT_INDEX,
            3,
            (id * 3) as u32,
        )
        .unwrap();

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
