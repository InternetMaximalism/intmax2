use std::{
    ops::{Add, Sub},
    str::FromStr,
    time::Duration,
};

use ethers::{abi::AbiEncode, types::H256};
use intmax2_cli::cli::{error::CliError, send::send_transfers};
use intmax2_client_sdk::{
    client::sync::utils::generate_salt, external_api::contract::utils::get_eth_balance,
};
use intmax2_zkp::{
    common::{generic_address::GenericAddress, signature::key_set::KeySet, transfer::Transfer},
    ethereum_types::{address::Address, u256::U256, u32limb_trait::U32LimbTrait},
};
use rand::Rng;
use serde::Deserialize;
use tests::{
    accounts::{derive_custom_keys, private_key_to_account, Account},
    deposit_native_token_with_error_handling,
    ethereum::transfer_eth_on_ethereum,
    get_eth_balance_on_intmax, wait_for_balance_synchronization,
    withdraw_directly_with_error_handling,
};

const ETH_TOKEN_INDEX: u32 = 0;
const RANDOM_ACTION_ACCOUNT_INDEX: u32 = 4;

#[derive(Debug, Deserialize)]
struct EnvVar {
    l1_rpc_url: String,
    transfer_admin_private_key: String,
}

#[derive(Debug)]
struct TestSystem {
    l1_rpc_url: String,
    admin_key: Account,
}

#[derive(Debug, Clone, Copy)]
enum Action {
    Deposit,
    Transfer,
    Withdrawal,
}

impl Action {
    fn random() -> Self {
        let actions = [Action::Deposit, Action::Transfer, Action::Withdrawal];
        actions[rand::thread_rng().gen_range(0..actions.len())]
    }
}

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

    async fn execute_random_action(
        &self,
        keys: &[Account],
    ) -> Result<(), Box<dyn std::error::Error>> {
        let action = Action::random();
        log::info!("Action: {:?}", action);

        let sender_key = keys[0];
        let recipient_key = keys[1];
        match action {
            Action::Deposit => self
                .execute_deposit(sender_key, recipient_key.intmax_key)
                .await
                .unwrap(),
            Action::Transfer => {
                self.execute_transfer(sender_key.intmax_key, recipient_key.intmax_key)
                    .await?
            }
            Action::Withdrawal => {
                self.execute_withdrawal(sender_key.intmax_key, recipient_key)
                    .await?
            }
        }

        Ok(())
    }

    async fn execute_deposit(
        &self,
        sender_key: Account,
        recipient_key: KeySet,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let sender_initial_balance =
            get_eth_balance(&self.l1_rpc_url, sender_key.eth_address).await?;
        log::info!("Sender's pubkey: {}", sender_key.eth_private_key);
        log::info!("Sender's initial balance: {}", sender_initial_balance);
        let recipient_initial_balance = get_eth_balance_on_intmax(recipient_key).await?;
        log::info!("Recipient's pubkey: {}", recipient_key.pubkey.to_hex());
        log::info!("Recipient's initial balance: {}", recipient_initial_balance);

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
        )
        .await?;
        log::info!("Deposit completed {}", sender_key.eth_address);

        // Check final balances
        let sender_final_balance =
            get_eth_balance(&self.l1_rpc_url, sender_key.eth_address).await?;
        log::info!("Sender's final balance: {}", sender_final_balance);
        let recipient_final_balance = get_eth_balance_on_intmax(recipient_key).await?;
        log::info!("Recipient's final balance: {}", recipient_final_balance);

        // Expected result: Recipient's balance should increase by only one transfer amount
        let expected_recipient_balance =
            recipient_initial_balance + U256::from_hex(&transfer_amount.encode_hex()).unwrap();

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

        Ok(())
    }

    async fn execute_transfer(
        &self,
        sender_key: KeySet,
        recipient_key: KeySet,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let sender_initial_balance = get_eth_balance_on_intmax(sender_key).await?;
        log::info!("Sender's pubkey: {}", sender_key.pubkey.to_hex());
        log::info!("Sender's initial balance: {}", sender_initial_balance);
        let recipient_initial_balance = get_eth_balance_on_intmax(recipient_key).await?;
        log::info!("Recipient's pubkey: {}", recipient_key.pubkey.to_hex());
        log::info!("Recipient's initial balance: {}", recipient_initial_balance);

        let transfer_amount = U256::from(10000);
        let fee = U256::from(1000);
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
        log::info!("Transaction Result {:?}", result);

        // Check final balances
        let sender_final_balance = get_eth_balance_on_intmax(sender_key).await?;
        log::info!("Sender's final balance: {}", sender_final_balance);
        let recipient_final_balance = get_eth_balance_on_intmax(recipient_key).await?;
        log::info!("Recipient's final balance: {}", recipient_final_balance);

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

        Ok(())
    }

    async fn execute_withdrawal(
        &self,
        sender_key: KeySet,
        recipient_account: Account,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let sender_initial_balance = get_eth_balance_on_intmax(sender_key).await?;
        log::info!("Sender's pubkey: {}", sender_key.pubkey.to_hex());
        log::info!("Sender's privkey: {}", sender_key.privkey);
        log::info!("Sender's initial balance: {}", sender_initial_balance);
        let recipient_initial_balance =
            get_eth_balance(&self.l1_rpc_url, recipient_account.eth_address).await?;
        log::info!("Recipient's initial balance: {}", recipient_initial_balance);

        let transfer_amount = U256::from(10);
        let fee = U256::from(1000);
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

            // return Err("Sender's balance is insufficient".into());
        }

        let to = Address::from_bytes_be(recipient_account.eth_address.as_bytes());
        log::info!("withdrawal recipient: {}", to);

        log::info!("Starting withdrawal {}", sender_key.pubkey);
        withdraw_directly_with_error_handling(sender_key, to, transfer_amount, ETH_TOKEN_INDEX)
            .await?;
        log::info!("Withdrawal completed {}", sender_key.pubkey);

        // Check final balances
        let sender_final_balance = get_eth_balance_on_intmax(sender_key).await?;
        log::info!("Sender's final balance: {}", sender_final_balance);
        let recipient_final_balance =
            get_eth_balance(&self.l1_rpc_url, recipient_account.eth_address).await?;
        log::info!("Recipient's final balance: {}", recipient_final_balance);

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

/// Double spend risk test: Test sending multiple transactions with the same nonce
async fn test_random_action() -> Result<(), Box<dyn std::error::Error>> {
    log::info!("Starting random action test");

    let master_mnemonic = std::env::var("MASTER_MNEMONIC").expect("MASTER_MNEMONIC must be set");
    let sender_keys = derive_custom_keys(&master_mnemonic, RANDOM_ACTION_ACCOUNT_INDEX, 2, 0)?;

    let test_system = TestSystem::new();
    test_system.execute_random_action(&sender_keys).await?;

    log::info!("test completed");
    Ok(())
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
    // let config = envy::from_env::<EnvVar>().unwrap();
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .init();

    test_random_action().await
}
