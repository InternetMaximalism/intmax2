use intmax2_interfaces::{
    api::store_vault_server::types::{MetaDataCursor, MetaDataCursorResponse},
    data::{deposit_data::DepositData, transfer_data::TransferData, tx_data::TxData},
    utils::key::ViewPair,
};

use crate::client::strategy::entry_status::HistoryEntry;

use super::{
    client::Client,
    error::ClientError,
    strategy::{deposit::fetch_deposit_info, transfer::fetch_transfer_info, tx::fetch_tx_info},
};

pub async fn fetch_deposit_history(
    client: &Client,
    view_pair: ViewPair,
    cursor: &MetaDataCursor,
) -> Result<(Vec<HistoryEntry<DepositData>>, MetaDataCursorResponse), ClientError> {
    // We don't need to check validity prover's sync status like in strategy
    // because fetching history is not a critical operation.
    let current_time = chrono::Utc::now().timestamp() as u64;
    let (history, cursor_response) = fetch_deposit_info(
        client.store_vault_server.as_ref(),
        client.validity_prover.as_ref(),
        &client.liquidity_contract,
        view_pair,
        current_time,
        &[],
        &[],
        cursor,
        client.config.deposit_timeout,
    )
    .await?;
    Ok((history, cursor_response))
}

pub async fn fetch_transfer_history(
    client: &Client,
    view_pair: ViewPair,
    cursor: &MetaDataCursor,
) -> Result<(Vec<HistoryEntry<TransferData>>, MetaDataCursorResponse), ClientError> {
    let current_time = chrono::Utc::now().timestamp() as u64;

    let (history, cursor_response) = fetch_transfer_info(
        client.store_vault_server.as_ref(),
        client.validity_prover.as_ref(),
        view_pair,
        current_time,
        &[],
        &[],
        cursor,
        client.config.tx_timeout,
    )
    .await?;

    Ok((history, cursor_response))
}

pub async fn fetch_tx_history(
    client: &Client,
    view_pair: ViewPair,
    cursor: &MetaDataCursor,
) -> Result<(Vec<HistoryEntry<TxData>>, MetaDataCursorResponse), ClientError> {
    let current_time = chrono::Utc::now().timestamp() as u64;
    let (history, cursor_response) = fetch_tx_info(
        client.store_vault_server.as_ref(),
        client.validity_prover.as_ref(),
        view_pair,
        current_time,
        &[],
        &[],
        cursor,
        client.config.tx_timeout,
    )
    .await?;

    Ok((history, cursor_response))
}
