use intmax2_interfaces::{
    api::{
        store_vault_server::{
            interface::StoreVaultClientInterface,
            types::{CursorOrder, MetaDataCursor, MetaDataCursorResponse},
        },
        validity_prover::interface::ValidityProverClientInterface,
    },
    data::{data_type::DataType, deposit_data::DepositData, user_data::ProcessStatus},
    utils::key::ViewPair,
};
use intmax2_zkp::ethereum_types::{bytes32::Bytes32, u32limb_trait::U32LimbTrait};

use crate::{
    client::strategy::entry_status::{EntryStatus, HistoryEntry},
    external_api::contract::liquidity_contract::LiquidityContract,
};

use super::{common::fetch_decrypt_validate, error::StrategyError};

#[allow(clippy::too_many_arguments)]
pub async fn fetch_deposit_info(
    store_vault_server: &dyn StoreVaultClientInterface,
    validity_prover: &dyn ValidityProverClientInterface,
    liquidity_contract: &LiquidityContract,
    view_pair: ViewPair,
    current_time: u64, // current timestamp for timeout checking
    included_digests: &[Bytes32],
    excluded_digests: &[Bytes32],
    cursor: &MetaDataCursor,
    deposit_timeout: u64,
) -> Result<(Vec<HistoryEntry<DepositData>>, MetaDataCursorResponse), StrategyError> {
    let mut all = Vec::new();
    let (data_with_meta, cursor_response) = fetch_decrypt_validate::<DepositData>(
        store_vault_server,
        view_pair.view,
        DataType::Deposit,
        included_digests,
        excluded_digests,
        cursor,
    )
    .await?;

    // Batch fetch deposit info for all valid deposits
    let pubkey_salt_hashes: Vec<_> = data_with_meta
        .iter()
        .map(|(_, deposit_data)| deposit_data.pubkey_salt_hash)
        .collect();
    let deposit_infos = validity_prover
        .get_deposit_info_batch(&pubkey_salt_hashes)
        .await?;

    // Process results and categorize deposits
    for ((meta, mut deposit_data), deposit_info) in data_with_meta.into_iter().zip(deposit_infos) {
        match deposit_info {
            Some(info) => {
                deposit_data.set_token_index(info.token_index);

                if let Some(block_number) = info.block_number {
                    // deposit is settled
                    all.push(HistoryEntry {
                        data: deposit_data,
                        status: EntryStatus::Settled(block_number),
                        meta,
                    });
                } else {
                    // deposit is not settled
                    let exists = liquidity_contract
                        .check_if_deposit_exists(info.deposit_id)
                        .await?;
                    if exists {
                        // deposit is not relayed to L2 yet
                        log::info!("Deposit {} is pending", meta.digest);
                        all.push(HistoryEntry {
                            data: deposit_data,
                            status: EntryStatus::Pending,
                            meta,
                        });
                    } else {
                        // deposit is canceled
                        log::info!(
                            "Deposit digest: {}, deposit_hash: {} is canceled",
                            meta.digest,
                            deposit_data.deposit_hash().unwrap()
                        );
                        all.push(HistoryEntry {
                            data: deposit_data,
                            status: EntryStatus::Timeout,
                            meta,
                        });
                    }
                }
            }
            None if meta.timestamp + deposit_timeout < current_time => {
                // Deposit has timed out
                all.push(HistoryEntry {
                    data: deposit_data,
                    status: EntryStatus::Timeout,
                    meta,
                });
            }
            None => {
                // Deposit is still pending
                log::info!("Deposit {} is pending", meta.digest);
                all.push(HistoryEntry {
                    data: deposit_data,
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

pub async fn fetch_all_unprocessed_deposit_info(
    store_vault_server: &dyn StoreVaultClientInterface,
    validity_prover: &dyn ValidityProverClientInterface,
    liquidity_contract: &LiquidityContract,
    view_pair: ViewPair,
    current_time: u64,
    process_status: &ProcessStatus,
    deposit_timeout: u64,
) -> Result<Vec<HistoryEntry<DepositData>>, StrategyError> {
    let mut cursor = MetaDataCursor {
        cursor: process_status.last_processed_meta_data.clone(),
        order: CursorOrder::Asc,
        limit: None,
    };
    let mut included_digests = process_status.pending_digests.clone(); // cleared after first fetch

    let mut all = Vec::new();
    loop {
        let (part, cursor_response) = fetch_deposit_info(
            store_vault_server,
            validity_prover,
            liquidity_contract,
            view_pair,
            current_time,
            &included_digests,
            &process_status.processed_digests,
            &cursor,
            deposit_timeout,
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
