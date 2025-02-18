use intmax2_client_sdk::{client::client::PaymentMemoEntry, external_api::indexer::IndexerClient};
use intmax2_interfaces::api::indexer::interface::IndexerClientInterface;
use intmax2_zkp::{
    common::{signature::key_set::KeySet, transfer::Transfer},
    constants::NUM_TRANSFERS_IN_TX,
    ethereum_types::u32limb_trait::U32LimbTrait,
};

use crate::{cli::client::get_client, env_var::EnvVar};

use super::error::CliError;

pub async fn send_transfers(
    key: KeySet,
    transfers: &[Transfer],
    payment_memos: Vec<PaymentMemoEntry>,
    fee_token_index: u32,
) -> Result<(), CliError> {
    if transfers.len() > NUM_TRANSFERS_IN_TX - 1 {
        return Err(CliError::TooManyTransfer(transfers.len()));
    }
    let env = envy::from_env::<EnvVar>()?;
    let client = get_client()?;
    // override block builder base url if it is set in the env
    let block_builder_url = if let Some(block_builder_base_url) = env.block_builder_base_url {
        block_builder_base_url.to_string()
    } else {
        // get block builder info
        let indexer = IndexerClient::new(&env.indexer_base_url.to_string());
        let block_builder_info = indexer.get_block_builder_info().await?;
        if block_builder_info.is_empty() {
            return Err(CliError::UnexpectedError(
                "Block builder info is empty".to_string(),
            ));
        }
        block_builder_info.first().unwrap().url.clone()
    };

    let fee_quote = client
        .quote_transfer_fee(&block_builder_url, key.pubkey, fee_token_index)
        .await?;
    if let Some(fee) = &fee_quote.fee {
        log::info!("beneficiary: {}", fee_quote.beneficiary.unwrap().to_hex());
        log::info!("Fee: {} (token# {})", fee.amount, fee.token_index);
    }
    if let Some(collateral_fee) = &fee_quote.collateral_fee {
        log::info!(
            "Collateral Fee: {} (token# {})",
            collateral_fee.amount,
            collateral_fee.token_index
        );
    }
    let memo = client
        .send_tx_request(
            &block_builder_url,
            key,
            transfers.to_vec(),
            payment_memos,
            fee_quote.beneficiary,
            fee_quote.fee,
            fee_quote.collateral_fee,
        )
        .await?;

    let is_registration_block = memo.is_registration_block;
    let tx = memo.tx;

    log::info!("Waiting for block builder to build the block");
    tokio::time::sleep(std::time::Duration::from_secs(
        env.block_builder_query_wait_time,
    ))
    .await;

    let proposal = client
        .query_proposal(&block_builder_url, key, is_registration_block, tx)
        .await?;

    log::info!("Finalizing tx");
    client
        .finalize_tx(&block_builder_url, key, &memo, &proposal)
        .await?;

    Ok(())
}
