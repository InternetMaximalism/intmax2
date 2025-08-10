use super::{common::fetch_decrypt_validate, error::StrategyError};
use crate::client::strategy::entry_status::{EntryStatus, HistoryEntry};
use intmax2_interfaces::{
    api::{
        store_vault_server::{
            interface::StoreVaultClientInterface,
            types::{CursorOrder, MetaDataCursor, MetaDataCursorResponse},
        },
        validity_prover::interface::ValidityProverClientInterface,
    },
    data::{data_type::DataType, tx_data::TxData, user_data::ProcessStatus},
    utils::key::ViewPair,
};
use intmax2_zkp::ethereum_types::{bytes32::Bytes32, u32limb_trait::U32LimbTrait as _};

#[allow(clippy::too_many_arguments)]
pub async fn fetch_tx_info(
    store_vault_server: &dyn StoreVaultClientInterface,
    validity_prover: &dyn ValidityProverClientInterface,
    view_pair: ViewPair,
    current_time: u64, // current timestamp for timeout checking
    included_digests: &[Bytes32],
    excluded_digests: &[Bytes32],
    cursor: &MetaDataCursor,
    tx_timeout: u64,
) -> Result<(Vec<HistoryEntry<TxData>>, MetaDataCursorResponse), StrategyError> {
    let mut all = Vec::new();
    let (data_with_meta, cursor_response) = fetch_decrypt_validate::<TxData>(
        store_vault_server,
        view_pair.view,
        DataType::Tx,
        included_digests,
        excluded_digests,
        cursor,
    )
    .await?;

    // Prepare batch request data
    let tx_tree_roots: Vec<_> = data_with_meta
        .iter()
        .map(|(_, tx_data)| tx_data.tx_tree_root)
        .collect();
    let block_numbers = validity_prover
        .get_block_number_by_tx_tree_root_batch(&tx_tree_roots)
        .await?;

    // Process results and categorize transactions
    for ((meta, tx_data), block_number) in data_with_meta.into_iter().zip(block_numbers) {
        match block_number {
            Some(block_number) => {
                // Transaction is settled
                all.push(HistoryEntry {
                    data: tx_data,
                    status: EntryStatus::Settled(block_number),
                    meta,
                });
            }
            None if meta.timestamp + tx_timeout < current_time => {
                // Transaction has timed out
                all.push(HistoryEntry {
                    data: tx_data,
                    status: EntryStatus::Timeout,
                    meta,
                });
            }
            None => {
                // Transaction is still pending
                log::info!("Tx {} is pending", meta.digest);
                all.push(HistoryEntry {
                    data: tx_data,
                    status: EntryStatus::Pending,
                    meta,
                });
            }
        }
    }

    // sort
    all.sort_by_key(|entry| {
        let HistoryEntry { meta, .. } = entry;
        (meta.timestamp, meta.digest.to_hex())
    });
    if cursor.order == CursorOrder::Desc {
        all.reverse();
    }

    Ok((all, cursor_response))
}

pub async fn fetch_all_unprocessed_tx_info(
    store_vault_server: &dyn StoreVaultClientInterface,
    validity_prover: &dyn ValidityProverClientInterface,
    view_pair: ViewPair,
    current_time: u64,
    process_status: &ProcessStatus,
    tx_timeout: u64,
) -> Result<Vec<HistoryEntry<TxData>>, StrategyError> {
    let mut cursor = MetaDataCursor {
        cursor: process_status.last_processed_meta_data.clone(),
        order: CursorOrder::Asc,
        limit: None,
    };
    let mut included_digests = process_status.pending_digests.clone(); // cleared after first fetch

    let mut all = Vec::new();
    loop {
        let (part, cursor_response) = fetch_tx_info(
            store_vault_server,
            validity_prover,
            view_pair,
            current_time,
            &included_digests,
            &process_status.processed_digests,
            &cursor,
            tx_timeout,
        )
        .await?;
        if !included_digests.is_empty() {
            included_digests = Vec::new(); // clear included_digests after first fetch
        }

        all.extend(part);
        if !cursor_response.has_more {
            break;
        }
        cursor.cursor = cursor_response.next_cursor;
    }

    Ok(all)
}
