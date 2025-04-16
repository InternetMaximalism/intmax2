use intmax2_interfaces::{
    api::store_vault_server::types::{MetaDataCursor, MetaDataCursorResponse},
    data::{
        deposit_data::DepositData,
        meta_data::{MetaData, MetaDataWithBlockNumber},
        transfer_data::TransferData,
        tx_data::TxData,
    },
};
use intmax2_zkp::{
    common::signature_content::key_set::KeySet,
    ethereum_types::{bytes32::Bytes32, u32limb_trait::U32LimbTrait},
};
use serde::{Deserialize, Serialize};

use super::{
    client::Client,
    error::ClientError,
    strategy::{deposit::fetch_deposit_info, transfer::fetch_transfer_info, tx::fetch_tx_info},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry<T> {
    pub data: T,
    pub status: EntryStatus,
    pub meta: MetaData,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EntryStatus {
    Settled(u32),   // Settled at block number but not processed yet
    Processed(u32), // Incorporated into the balance proof
    Pending,        // Not settled yet
    Timeout,        // Timed out
}

impl EntryStatus {
    pub fn from_settled(processed_digests: &[Bytes32], meta: MetaDataWithBlockNumber) -> Self {
        if processed_digests.contains(&meta.meta.digest) {
            EntryStatus::Processed(meta.block_number)
        } else {
            EntryStatus::Settled(meta.block_number)
        }
    }
}

pub async fn fetch_deposit_history(
    client: &Client,
    key: KeySet,
    cursor: &MetaDataCursor,
) -> Result<(Vec<HistoryEntry<DepositData>>, MetaDataCursorResponse), ClientError> {
    // We don't need to check validity prover's sync status like in strategy
    // because fetching history is not a critical operation.
    let current_time = chrono::Utc::now().timestamp() as u64;
    let user_data = client.get_user_data(key).await?;

    let mut history = Vec::new();
    let (all_deposit_info, cursor_response) = fetch_deposit_info(
        client.store_vault_server.as_ref(),
        client.validity_prover.as_ref(),
        &client.liquidity_contract,
        key,
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
            status: EntryStatus::from_settled(
                &user_data.deposit_status.processed_digests,
                meta.clone(),
            ),
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
    key: KeySet,
    cursor: &MetaDataCursor,
) -> Result<(Vec<HistoryEntry<TransferData>>, MetaDataCursorResponse), ClientError> {
    let current_time = chrono::Utc::now().timestamp() as u64;
    let user_data = client.get_user_data(key).await?;

    let mut history = Vec::new();
    let (all_transfers_info, cursor_response) = fetch_transfer_info(
        client.store_vault_server.as_ref(),
        client.validity_prover.as_ref(),
        key,
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
    key: KeySet,
    cursor: &MetaDataCursor,
) -> Result<(Vec<HistoryEntry<TxData>>, MetaDataCursorResponse), ClientError> {
    let current_time = chrono::Utc::now().timestamp() as u64;
    let user_data = client.get_user_data(key).await?;

    let mut history = Vec::new();
    let (all_tx_info, cursor_response) = fetch_tx_info(
        client.store_vault_server.as_ref(),
        client.validity_prover.as_ref(),
        key,
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
