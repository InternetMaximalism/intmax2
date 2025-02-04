use std::time::Duration;

use ethers::{
    signers::coins_bip39::{English, Mnemonic},
    types::H256,
    utils::hex,
};
use futures::future::join_all;
use intmax2_cli::{
    cli::{
        error::CliError,
        get::balance,
        send::{transfer, TransferInput},
    },
    format::privkey_to_keyset,
};
use intmax2_client_sdk::client::{
    error::ClientError, key_from_eth::generate_intmax_account_from_eth_key,
    strategy::error::StrategyError, sync::error::SyncError,
};
use intmax2_interfaces::api::error::ServerError;
use intmax2_zkp::{
    common::signature::key_set::KeySet, ethereum_types::u32limb_trait::U32LimbTrait,
};
use serde::Deserialize;
use tiny_hderive::bip32::ExtendedPrivKey;

#[derive(Debug, Clone, Deserialize)]
pub struct EnvVar {
    pub master_mnemonic: String,
    pub private_key: String,
    pub num_of_recipients: Option<u32>,
    pub recipient_offset: Option<u32>,
    pub balance_prover_base_url: String,
}

#[derive(Debug, Clone, Default)]
pub struct MnemonicToPrivateKeyOptions {
    pub account_index: u32,
    pub address_index: u32,
}

const ETH_TOKEN_INDEX: u32 = 0;

fn mnemonic_to_private_key(
    mnemonic_phrase: &str,
    options: MnemonicToPrivateKeyOptions,
) -> Result<H256, Box<dyn std::error::Error>> {
    let mnemonic = Mnemonic::<English>::new_from_phrase(mnemonic_phrase)?;
    let seed = mnemonic.to_seed(None)?;

    let account_index = options.account_index;
    let address_index = options.address_index;
    let hd_path = format!("m/44'/60'/{account_index}'/0/{address_index}");

    let master_key = ExtendedPrivKey::derive(&seed, hd_path.as_str())
        .map_err(|e| format!("Failed to derive private key: {e:?}"))?;
    let private_key_bytes = master_key.secret();

    Ok(H256(private_key_bytes))
}

async fn wait_for_balance_synchronization(intmax_sender: KeySet) -> Result<(), CliError> {
    loop {
        let result = balance(intmax_sender).await;
        match result {
            Ok(_) => return Ok(()),
            Err(CliError::SyncError(SyncError::StrategyError(StrategyError::PendingTxError(
                _,
            )))) => {
                println!("Pending transaction. Waiting for the balance to be updated...");
            }
            Err(CliError::SyncError(SyncError::ValidityProverIsNotSynced(_))) => {
                println!("Validity prover is not synced. Waiting for the balance to be updated...");
            }
            Err(CliError::SyncError(SyncError::ServerError(ServerError::ServerError(
                500,
                message,
                _,
                _,
            )))) => {
                println!("{message}. Waiting for the balance to be updated...");
            }
            Err(CliError::ClientError(ClientError::ServerError(ServerError::ServerError(
                status,
                message,
                url,
                query,
            )))) => {
                println!("Server error status={status}, url={url}, query={query}");
                println!("{message}. Waiting for the balance to be updated...");
            }
            Err(e) => {
                return Err(e);
            }
        }

        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

async fn transfer_with_error_handling(
    key: KeySet,
    transfer_inputs: &[TransferInput],
) -> Option<CliError> {
    let result = transfer(key, transfer_inputs).await;

    match result {
        Ok(_) => None,
        Err(e) => Some(e),
    }
}

// TODO: retry for rate limit

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

    let mut intmax_recipients = vec![];
    for address_index in 0..num_of_recipients {
        let options = MnemonicToPrivateKeyOptions {
            account_index: 0,
            address_index: offset + address_index,
        };
        let private_key = mnemonic_to_private_key(&master_mnemonic_phrase, options)?;
        let key = generate_intmax_account_from_eth_key(private_key);
        intmax_recipients.push(key);
    }

    // 1. sender -> recipients (bulk-transfer)
    let transfers = intmax_recipients
        .iter()
        .map(|recipient| TransferInput {
            recipient: recipient.pubkey.to_hex(),
            amount: 10000u128,
            token_index: ETH_TOKEN_INDEX,
        })
        .collect::<Vec<_>>();
    transfer(intmax_sender, &transfers).await?;

    // println!("Transferred to recipients. Waiting for the balance to be updated...");
    // tokio::time::sleep(Duration::from_secs(30)).await;

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

    let mut intmax_recipients = vec![];
    for address_index in 0..num_of_recipients {
        let options = MnemonicToPrivateKeyOptions {
            account_index: 0,
            address_index: offset + address_index,
        };
        let private_key = mnemonic_to_private_key(&master_mnemonic_phrase, options)?;
        let key = generate_intmax_account_from_eth_key(private_key);
        intmax_recipients.push(key);
    }

    wait_for_balance_synchronization(intmax_sender).await?;
    println!("Balance updated. Proceeding to the next step.");

    for (i, recipient) in intmax_recipients.iter().enumerate() {
        println!("Recipient ({}/{})", i + 1, num_of_recipients);
        wait_for_balance_synchronization(*recipient).await?;
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

    // For example, MNEMONIC="park remain person kitchen mule spell knee armed position rail grid ankle"
    let master_mnemonic_phrase = config.master_mnemonic;
    let num_of_recipients = config.num_of_recipients.unwrap_or(1);
    log::debug!("Number of recipients: {}", num_of_recipients);
    if num_of_recipients > 128 {
        return Err("Number of recipients must be less than or equal to 128".into());
    }

    let offset = 0;

    let mut intmax_recipients = vec![];
    for address_index in 0..num_of_recipients {
        let options = MnemonicToPrivateKeyOptions {
            account_index: 0,
            address_index: offset + address_index,
        };
        let private_key = mnemonic_to_private_key(&master_mnemonic_phrase, options)?;
        let key = generate_intmax_account_from_eth_key(private_key);
        intmax_recipients.push(key);
    }

    let intmax_recipient = {
        let options = MnemonicToPrivateKeyOptions {
            account_index: 1,
            address_index: 0,
        };
        let private_key = mnemonic_to_private_key(&master_mnemonic_phrase, options)?;

        generate_intmax_account_from_eth_key(private_key)
    };

    // 2. recipients -> sender (simultaneously)
    let transfer_input = TransferInput {
        recipient: intmax_recipient.pubkey.to_hex(),
        amount: 100u128,
        token_index: ETH_TOKEN_INDEX,
    };
    let transfers = [transfer_input];

    log::info!("Transferring from recipients to sender...");
    tokio::time::sleep(Duration::from_secs(1)).await;

    let futures = intmax_recipients.iter().map(|recipient| {
        let future = transfer_with_error_handling(*recipient, &transfers);

        future
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
