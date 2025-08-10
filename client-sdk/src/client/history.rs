use intmax2_interfaces::{
    api::store_vault_server::types::{MetaDataCursor, MetaDataCursorResponse},
    data::{deposit_data::DepositData, transfer_data::TransferData, tx_data::TxData},
    utils::key::ViewPair,
};
use intmax2_zkp::ethereum_types::u32limb_trait::U32LimbTrait;

use crate::client::strategy::entry_status::{EntryStatus, HistoryEntry};

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
    let mut history = Vec::new();
    let (all_deposit_info, cursor_response) = fetch_deposit_info(
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
    for (meta, settled) in all_deposit_info.settled {
        history.push(HistoryEntry {
            data: settled,
            status: EntryStatus::Settled(meta.block_number),
            meta: meta.meta,
        });
    }
    for (meta, pending) in all_deposit_info.pending {
        history.push(HistoryEntry {
            data: pending,
            status: EntryStatus::Pending,
            meta,
        });
    }
    for (meta, timeout) in all_deposit_info.timeout {
        history.push(HistoryEntry {
            data: timeout,
            status: EntryStatus::Timeout,
            meta,
        });
    }

    history.sort_by_key(|entry| {
        let HistoryEntry { meta, .. } = entry;
        (meta.timestamp, meta.digest.to_hex())
    });

    Ok((history, cursor_response))
}

pub async fn fetch_transfer_history(
    client: &Client,
    view_pair: ViewPair,
    cursor: &MetaDataCursor,
) -> Result<(Vec<HistoryEntry<TransferData>>, MetaDataCursorResponse), ClientError> {
    let current_time = chrono::Utc::now().timestamp() as u64;
    let user_data = client.get_user_data(view_pair).await?;

    let mut history = Vec::new();
    let (all_transfers_info, cursor_response) = fetch_transfer_info(
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
    for (meta, settled) in all_transfers_info.settled {
        history.push(HistoryEntry {
            data: settled,
            status: EntryStatus::from_settled(
                &user_data.transfer_status.processed_digests,
                meta.clone(),
            ),
            meta: meta.meta,
        });
    }
    for (meta, pending) in all_transfers_info.pending {
        history.push(HistoryEntry {
            data: pending,
            status: EntryStatus::Pending,
            meta: meta.clone(),
        });
    }
    for (meta, timeout) in all_transfers_info.timeout {
        history.push(HistoryEntry {
            data: timeout,
            status: EntryStatus::Timeout,
            meta: meta.clone(),
        });
    }

    history.sort_by_key(|entry| {
        let HistoryEntry { meta, .. } = entry;
        (meta.timestamp, meta.digest.to_hex())
    });

    Ok((history, cursor_response))
}

pub async fn fetch_tx_history(
    client: &Client,
    view_pair: ViewPair,
    cursor: &MetaDataCursor,
) -> Result<(Vec<HistoryEntry<TxData>>, MetaDataCursorResponse), ClientError> {
    let current_time = chrono::Utc::now().timestamp() as u64;
    let user_data = client.get_user_data(view_pair).await?;

    let mut history = Vec::new();
    let (all_tx_info, cursor_response) = fetch_tx_info(
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
    for (meta, settled) in all_tx_info.settled {
        history.push(HistoryEntry {
            data: settled,
            status: EntryStatus::from_settled(&user_data.tx_status.processed_digests, meta.clone()),
            meta: meta.meta.clone(),
        });
    }
    for (meta, pending) in all_tx_info.pending {
        history.push(HistoryEntry {
            data: pending,
            status: EntryStatus::Pending,
            meta,
        });
    }
    for (meta, timeout) in all_tx_info.timeout {
        history.push(HistoryEntry {
            data: timeout,
            status: EntryStatus::Timeout,
            meta,
        });
    }

    history.sort_by_key(|entry| {
        let HistoryEntry { meta, .. } = entry;
        (meta.timestamp, meta.digest.to_hex())
    });

    Ok((history, cursor_response))
}
