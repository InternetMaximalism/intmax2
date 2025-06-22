use std::time::Duration;

use intmax2_client_sdk::{
    client::{
        client::Client,
        strategy::tx_status::TxStatus,
        types::{PaymentMemoEntry, TransferRequest},
    },
    external_api::{indexer::IndexerClient, utils::time::sleep_for},
};
use intmax2_interfaces::{
    api::indexer::interface::IndexerClientInterface,
    utils::{
        key::{KeyPair, PublicKey},
        signature::current_time,
    },
};
use intmax2_zkp::{
    common::block_builder::BlockProposal, constants::NUM_TRANSFERS_IN_TX,
    ethereum_types::bytes32::Bytes32,
};
use tokio::time::Instant;

use crate::{
    cli::{client::get_client, fee_cap::validate_fee_cap},
    env_var::EnvVar,
};

use super::error::CliError;

const TX_STATUS_POLLING_INTERVAL: u64 = 5;
const BLOCK_SYNC_MARGIN: u64 = 30;

pub async fn send_transfers(
    key_pair: KeyPair,
    transfer_requests: &[TransferRequest],
    payment_memos: Vec<PaymentMemoEntry>,
    fee_token_index: u32,
    wait: bool,
) -> Result<(), CliError> {
    let spend_pub = key_pair.spend.to_public_key();

    if transfer_requests.len() > NUM_TRANSFERS_IN_TX - 1 {
        return Err(CliError::TooManyTransfer(transfer_requests.len()));
    }

    let env = envy::from_env::<EnvVar>()?;
    let block_builder_url = get_block_builder_url(&env).await?;
    log::info!("Block Builder URL: {block_builder_url}",);

    let client = get_client()?;
    let fee_quote = client
        .quote_transfer_fee(&block_builder_url, spend_pub.0, fee_token_index)
        .await?;
    if let Some(fee) = &fee_quote.fee {
        let fee_caps = if fee_quote.is_registration_block {
            &env.max_registoration_fee
        } else {
            &env.max_non_registoration_fee
        };
        validate_fee_cap(fee, fee_caps)?;
        log::info!("beneficiary: {}", fee_quote.beneficiary);
        log::info!("Fee: {} (token# {})", fee.amount, fee.token_index);
    }
    if let Some(collateral_fee) = &fee_quote.collateral_fee {
        let collateral_fee_caps = if fee_quote.is_registration_block {
            &env.max_registration_collateral_fee
        } else {
            &env.max_non_registration_collateral_fee
        };
        validate_fee_cap(collateral_fee, collateral_fee_caps)?;
        log::info!(
            "Collateral Fee: {} (token# {})",
            collateral_fee.amount,
            collateral_fee.token_index
        );
    }
    let memo = client
        .send_tx_request(
            &block_builder_url,
            key_pair,
            transfer_requests,
            &payment_memos,
            &fee_quote,
        )
        .await?;

    log::info!("Waiting for block builder to build the block...");
    tokio::time::sleep(std::time::Duration::from_secs(
        env.block_builder_query_wait_time,
    ))
    .await;

    let proposal = retry_async(
        || client.query_proposal(&block_builder_url, &memo.request_id),
        0,
        5,
        "Query_proposal",
    )
    .await
    .map_err(|_| CliError::FailedToGetProposal)?;

    log::info!("Finalizing tx");
    let result = client
        .finalize_tx(&block_builder_url, key_pair, &memo, &proposal)
        .await?;

    if wait {
        wait_for_tx_status(&client, spend_pub, &result.tx_tree_root, &proposal).await?;
    }

    Ok(())
}

async fn get_block_builder_url(env: &EnvVar) -> Result<String, CliError> {
    // override block builder base url if it is set in the env
    if let Some(ref url) = env.block_builder_base_url {
        Ok(url.to_string())
    } else {
        let indexer = IndexerClient::new(&env.indexer_base_url);
        let block_builder_info = indexer.get_block_builder_info().await?;
        Ok(block_builder_info.url)
    }
}

pub async fn retry_async<F, Fut, T, E>(
    mut operation: F,
    mut attempts: i32,
    max_retries: i32,
    label: &str,
) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    loop {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                attempts += 1;
                if attempts >= max_retries {
                    log::error!("{label} failed after {max_retries} attempts: {e}");
                    return Err(e);
                }
                log::warn!("{label} failed (attempt {attempts}): {e}. Retrying...");
                sleep_for(2).await;
            }
        }
    }
}

async fn wait_for_tx_status(
    client: &Client,
    spend_pub: PublicKey,
    tx_tree_root: &Bytes32,
    proposal: &BlockProposal,
) -> Result<(), CliError> {
    let expiry: u64 = proposal.block_sign_payload.expiry.into();
    let expiry_with_margin = if expiry > 0 {
        expiry + BLOCK_SYNC_MARGIN
    } else {
        chrono::Utc::now().timestamp() as u64 + BLOCK_SYNC_MARGIN
    };

    log::info!("Waiting for the block to be finalized");

    let deadline = Instant::now() + Duration::from_secs(expiry_with_margin - current_time());

    loop {
        if Instant::now() >= deadline {
            log::error!("tx expired");
            break;
        }

        let status = client.get_tx_status(spend_pub, *tx_tree_root).await?;
        match status {
            TxStatus::Pending => log::info!("tx pending"),
            TxStatus::Success => {
                log::info!("tx success");
                break;
            }
            TxStatus::Failed(reason) => {
                log::error!("tx failed: {reason}");
                break;
            }
        }

        sleep_for(TX_STATUS_POLLING_INTERVAL).await;
    }

    Ok(())
}
