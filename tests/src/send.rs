use crate::{config::TestConfig, utils::get_block_builder_url};
use anyhow::Context as _;
use intmax2_client_sdk::{
    client::{
        client::{Client, PaymentMemoEntry},
        strategy::tx_status::TxStatus,
    },
    external_api::utils::time::sleep_for,
};
use intmax2_zkp::{
    common::{signature_content::key_set::KeySet, transfer::Transfer},
    constants::NUM_TRANSFERS_IN_TX,
};

pub async fn send_transfers(
    config: &TestConfig,
    client: &Client,
    key: KeySet,
    transfers: &[Transfer],
    payment_memos: &[PaymentMemoEntry],
    fee_token_index: u32,
) -> anyhow::Result<()> {
    // override block builder base url if it is set in the env
    let block_builder_url = get_block_builder_url(&config.indexer_base_url).await?;
    let fee_quote = client
        .quote_transfer_fee(&block_builder_url, key.pubkey, fee_token_index)
        .await?;

    client
        .await_tx_sendable(key, transfers, &fee_quote)
        .await
        .context("Failed to get tx sendable")?;

    if transfers.len() > NUM_TRANSFERS_IN_TX - 1 {
        anyhow::bail!("Too many transfers: {}", transfers.len());
    }

    let memo = client
        .send_tx_request(
            &block_builder_url,
            key,
            transfers,
            payment_memos,
            &fee_quote,
        )
        .await?;

    log::info!("Waiting for block builder to build the block");
    tokio::time::sleep(std::time::Duration::from_secs(
        config.block_builder_query_wait_time,
    ))
    .await;

    let proposal = client
        .query_proposal(&block_builder_url, &memo.request_id)
        .await?;

    log::info!("Finalizing tx");
    let result = client
        .finalize_tx(&block_builder_url, key, &memo, &proposal)
        .await?;

    let expiry: u64 = proposal.block_sign_payload.expiry.into();
    let expiry_with_margin = if expiry > 0 {
        expiry + config.block_sync_margin
    } else {
        chrono::Utc::now().timestamp() as u64 + config.block_sync_margin
    };

    log::info!("Waiting for the block to be finalized");
    loop {
        if expiry_with_margin < chrono::Utc::now().timestamp() as u64 {
            anyhow::bail!("tx expired");
        }
        let status = client
            .get_tx_status(key.pubkey, result.tx_tree_root)
            .await?;
        match status {
            TxStatus::Pending => {
                log::info!("tx pending");
            }
            TxStatus::Success => {
                log::info!("tx success");
                break;
            }
            TxStatus::Failed(reason) => {
                anyhow::bail!("tx failed: {}", reason);
            }
        }
        sleep_for(config.tx_status_check_interval).await;
    }
    Ok(())
}
