use std::time::Duration;

use ethers::{types::H256, utils::hex};
use futures::future::join_all;
use intmax2_cli::{
    cli::send::{transfer, TransferInput},
    format::privkey_to_keyset,
};
use intmax2_client_sdk::client::key_from_eth::generate_intmax_account_from_eth_key;
use intmax2_zkp::ethereum_types::u32limb_trait::U32LimbTrait;
use tests::{
    derive_intmax_keys, mnemonic_to_private_key, transfer_with_error_handling,
    wait_for_balance_synchronization, EnvVar, MnemonicToPrivateKeyOptions,
};

const ETH_TOKEN_INDEX: u32 = 0;
const NUM_TRANSFER_LOOPS: usize = 2;

#[tokio::test]
async fn test_bulk_transfers() -> Result<(), Box<dyn std::error::Error>> {
    dotenv::dotenv()?;
    dotenv::from_path("../cli/.env")?;
    let config = envy::from_env::<EnvVar>().unwrap();
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .init();

    let private_key = H256::from_slice(&hex::decode(config.private_key)?);
    let intmax_sender = privkey_to_keyset(private_key);
    println!("Sender: {}", private_key);
    println!("pubkey: {}", intmax_sender.pubkey.to_hex());

    // For example, MNEMONIC="park remain person kitchen mule spell knee armed position rail grid ankle"
    let master_mnemonic_phrase = config.master_mnemonic;
    let num_of_recipients = config.num_of_recipients.unwrap_or(1);
    println!("Number of recipients: {}", num_of_recipients);
    if num_of_recipients > 64 {
        return Err("Number of recipients must be less than or equal to 64".into());
    }

    let offset = config.recipient_offset.unwrap_or(0);
    println!("Recipient offset: {}", offset);

    let intmax_recipients = derive_intmax_keys(&master_mnemonic_phrase, num_of_recipients, offset)?;

    // sender -> multiple recipients (bulk-transfer)
    let transfers = intmax_recipients
        .iter()
        .map(|recipient| TransferInput {
            recipient: recipient.pubkey.to_hex(),
            amount: 1000000000u128,
            token_index: ETH_TOKEN_INDEX,
        })
        .collect::<Vec<_>>();
    transfer(intmax_sender, &transfers).await?;

    Ok(())
}

#[tokio::test]
async fn test_sync_balance() -> Result<(), Box<dyn std::error::Error>> {
    dotenv::dotenv()?;
    dotenv::from_path("../cli/.env")?;
    let config = envy::from_env::<EnvVar>().unwrap();
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .init();

    let private_key = H256::from_slice(&hex::decode(config.private_key)?);
    let intmax_sender = privkey_to_keyset(private_key);
    println!("Sender: {}", private_key);
    println!("pubkey: {}", intmax_sender.pubkey.to_hex());

    // For example, MNEMONIC="park remain person kitchen mule spell knee armed position rail grid ankle"
    let master_mnemonic_phrase = config.master_mnemonic;
    let num_of_recipients = config.num_of_recipients.unwrap_or(1);
    println!("Number of recipients: {}", num_of_recipients);

    let offset = 0;
    let intmax_recipients = derive_intmax_keys(&master_mnemonic_phrase, num_of_recipients, offset)?;

    wait_for_balance_synchronization(intmax_sender, Duration::from_secs(5)).await?;
    println!("Balance updated. Proceeding to the next step.");

    for (i, recipient) in intmax_recipients.iter().enumerate() {
        println!("Recipient ({}/{})", i + 1, num_of_recipients);
        wait_for_balance_synchronization(*recipient, Duration::from_secs(5)).await?;
    }
    println!("Balance updated. Proceeding to the next step.");

    {
        let options = MnemonicToPrivateKeyOptions {
            account_index: 1,
            address_index: 0,
        };
        let private_key = mnemonic_to_private_key(&master_mnemonic_phrase, options)?;
        let key = generate_intmax_account_from_eth_key(private_key);
        wait_for_balance_synchronization(key, Duration::from_secs(5)).await?;
    }

    Ok(())
}

#[tokio::test]
async fn test_block_generation_included_many_senders() -> Result<(), Box<dyn std::error::Error>> {
    dotenv::dotenv()?;
    dotenv::from_path("../cli/.env")?;
    let config = envy::from_env::<EnvVar>().unwrap();
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .init();

    log::debug!(
        "block_builder_base_url: {:?}",
        config.balance_prover_base_url
    );

    let master_mnemonic_phrase = config.master_mnemonic;
    let num_of_recipients = config.num_of_recipients.unwrap_or(1);
    log::debug!("Number of recipients: {}", num_of_recipients);
    if num_of_recipients > 128 {
        return Err("Number of recipients must be less than or equal to 128".into());
    }

    let offset = 0;
    let intmax_senders = derive_intmax_keys(&master_mnemonic_phrase, num_of_recipients, offset)?;

    let intmax_recipient = {
        let options = MnemonicToPrivateKeyOptions {
            account_index: 1,
            address_index: 0,
        };
        let private_key = mnemonic_to_private_key(&master_mnemonic_phrase, options)?;

        generate_intmax_account_from_eth_key(private_key)
    };

    // multiple senders -> receiver (simultaneously)
    let transfer_input = TransferInput {
        recipient: intmax_recipient.pubkey.to_hex(),
        amount: 10u128,
        token_index: ETH_TOKEN_INDEX,
    };
    let transfers = [transfer_input];

    log::info!("Transferring from recipients to sender...");
    tokio::time::sleep(Duration::from_secs(1)).await;

    let futures = intmax_senders.iter().map(|sender| async {
        transfer_with_error_handling(*sender, &transfers, NUM_TRANSFER_LOOPS)
            .await
            .err()
    });

    let errors = join_all(futures).await;
    for (i, error) in errors.iter().enumerate() {
        if let Some(e) = error {
            log::error!(
                "Recipient ({}/{}) failed: {:?}",
                i + 1,
                num_of_recipients,
                e
            );
        } else {
            log::info!("Recipient ({}/{}) succeeded", i + 1, num_of_recipients);
        }
    }

    Ok(())
}
