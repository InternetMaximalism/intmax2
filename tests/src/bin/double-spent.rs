use intmax2_cli::cli::send::send_transfers;
use intmax2_client_sdk::client::sync::utils::generate_salt;
use intmax2_zkp::{
    common::{generic_address::GenericAddress, signature::key_set::KeySet, transfer::Transfer},
    ethereum_types::{u256::U256, u32limb_trait::U32LimbTrait},
};
use num_bigint::BigUint;
use std::time::Duration;
use tests::{accounts::derive_custom_intmax_keys, get_eth_balance_on_intmax};

const ETH_TOKEN_INDEX: u32 = 0;
const DOUBLE_SPENT_ACCOUNT_INDEX: u32 = 4;

/// Double spend risk test: Test sending multiple transactions with the same nonce
pub async fn test_double_spend() -> Result<(), Box<dyn std::error::Error>> {
    log::info!("Starting double spend test");

    // Prepare test keys (using existing functions)
    let master_mnemonic = std::env::var("MASTER_MNEMONIC").expect("MASTER_MNEMONIC must be set");

    // Create INTMAX keys for sender and recipient
    let sender_keys =
        derive_custom_intmax_keys(&master_mnemonic, DOUBLE_SPENT_ACCOUNT_INDEX, 1, 0)?;
    let recipient_keys =
        derive_custom_intmax_keys(&master_mnemonic, DOUBLE_SPENT_ACCOUNT_INDEX, 1, 100)?;

    let sender_key = sender_keys[0];
    println!("sender_key: {}", sender_key.privkey);
    let recipient_key = recipient_keys[0];

    log::info!("Sender: {}", sender_key.pubkey.to_hex());
    log::info!("Recipient: {}", recipient_key.pubkey.to_hex());

    // Check sender's initial balance
    let sender_initial_balance = get_eth_balance_on_intmax(sender_key).await?;
    log::info!("Sender's initial balance: {}", sender_initial_balance);

    // Abort test if balance is insufficient
    if sender_initial_balance.lt(&U256::from(1000000000)) {
        log::error!(
            "Sender's balance is insufficient. Pubkey: {}, Balance: {}",
            sender_key.pubkey.to_hex(),
            sender_initial_balance
        );
        return Err("Sender's balance is insufficient".into());
    }

    // Check recipient's initial balance
    let recipient_initial_balance = get_eth_balance_on_intmax(recipient_key).await?;
    log::info!("Recipient's initial balance: {}", recipient_initial_balance);

    // Execute double spend test
    let transfer_amount = U256::from(10000000); // Test transfer amount
    execute_double_spend_test(sender_key, recipient_key, transfer_amount).await?;

    // Verify results
    // log::info!("Double spend test result: {:?}", transfer_result);

    // Check final balances
    let sender_final_balance = get_eth_balance_on_intmax(sender_key).await?;
    let recipient_final_balance = get_eth_balance_on_intmax(recipient_key).await?;

    log::info!("Sender's final balance: {}", sender_final_balance);
    log::info!("Recipient's final balance: {}", recipient_final_balance);

    // Expected result: Recipient's balance should increase by only one transfer amount
    let expected_recipient_balance = recipient_initial_balance + transfer_amount;

    if recipient_final_balance == expected_recipient_balance {
        log::info!("Only one of the two transfer transactions was processed");
    } else if recipient_final_balance
        == recipient_initial_balance + transfer_amount + transfer_amount
    {
        log::error!("Both transfer transactions were processed (double spend occurred)");
        return Err("Double spend occurred".into());
    } else {
        log::warn!("Recipient's balance does not match expected value");
        log::warn!(
            "Expected: {}, Actual: {}",
            expected_recipient_balance,
            recipient_final_balance
        );
    }

    log::info!("Double spend test completed");
    Ok(())
}

/// Test sending two transactions with the same nonce simultaneously
async fn execute_double_spend_test(
    sender: KeySet,
    recipient: KeySet,
    amount: U256,
) -> anyhow::Result<()> {
    let recipient_before = get_eth_balance_on_intmax(recipient)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to get recipient balance: {}", e))?;

    let salt = generate_salt();
    log::info!("Using salt: {:?}", salt);

    let transfer1 = Transfer {
        recipient: GenericAddress::from_pubkey(recipient.pubkey),
        amount,
        token_index: ETH_TOKEN_INDEX,
        salt, // Using the same salt
    };

    let transfer2 = Transfer {
        recipient: GenericAddress::from_pubkey(recipient.pubkey),
        amount,
        token_index: ETH_TOKEN_INDEX,
        salt, // Using the same salt
    };

    // Send two transactions in parallel
    let sender_clone = sender;
    let handle1 = tokio::task::spawn_blocking(move || {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async move {
            log::info!("Transaction 1: Starting");
            let result = send_transfers(sender, &[transfer1], vec![], ETH_TOKEN_INDEX, true).await;
            log::info!("Transaction 1: Result {:?}", result);
            result
        })
    });

    // Add a small delay before sending the second transaction
    tokio::time::sleep(Duration::from_millis(100)).await;

    let handle2 = tokio::task::spawn_blocking(move || {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async move {
            log::info!("Transaction 2: Starting");
            let result =
                send_transfers(sender_clone, &[transfer2], vec![], ETH_TOKEN_INDEX, true).await;
            log::info!("Transaction 2: Result {:?}", result);
            result
        })
    });

    let result1 = handle1.await?;
    let result2 = handle2.await?;

    log::info!("Transactions sent, waiting for balance updates...");
    tokio::time::sleep(Duration::from_secs(30)).await;

    // Check balance after transactions
    let recipient_after = get_eth_balance_on_intmax(recipient)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to get recipient balance: {}", e))?;
    log::info!(
        "Recipient balance change: {} -> {}",
        recipient_before,
        recipient_after
    );

    let _success_count = match (&result1, &result2) {
        (Ok(_), Ok(_)) => 2,
        (Ok(_), Err(_)) => 1,
        (Err(_), Ok(_)) => 1,
        (Err(_), Err(_)) => 0,
    };

    let _actual_transfers = if recipient_after > recipient_before {
        let transfer_amount =
            BigUint::from_bytes_be(&(recipient_after - recipient_before).to_bytes_be())
                / BigUint::from_bytes_be(&amount.to_bytes_be());
        let digits = transfer_amount.to_u64_digits();
        if digits.len() > 1 {
            anyhow::bail!("Transfer count is too large");
        }

        digits[0]
    } else {
        0
    };

    // Ok(DoubleSpendResult {
    //     tx1_successful: result1.is_ok(),
    //     tx2_successful: result2.is_ok(),
    //     reported_success_count: success_count,
    //     actual_transfers,
    //     balance_before: recipient_before,
    //     balance_after: recipient_after,
    // })
    Ok(())
}

// #[derive(Debug)]
// struct DoubleSpendResult {
//     tx1_successful: bool,
//     tx2_successful: bool,
//     reported_success_count: u64,
//     actual_transfers: u64,
//     balance_before: U256,
//     balance_after: U256,
// }

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let current_dir = std::env::current_dir()?;
    let env_path = current_dir.join("../tests/.env");
    println!("env_path: {}", env_path.to_string_lossy());
    dotenv::from_path(env_path)?;
    let cli_env_path = current_dir.join("../cli/.env");
    println!("cli_env_path: {}", cli_env_path.to_string_lossy());
    dotenv::from_path(cli_env_path)?;
    // let config = envy::from_env::<EnvVar>().unwrap();
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .init();

    test_double_spend().await
}
