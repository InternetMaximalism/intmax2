use intmax2_interfaces::{
    api::{
        store_vault_server::{
            interface::{DataType, StoreVaultClientInterface},
            types::{CursorOrder, MetaDataCursor, MetaDataCursorResponse},
        },
        validity_prover::interface::ValidityProverClientInterface,
    },
    data::{
        deposit_data::DepositData,
        meta_data::{MetaData, MetaDataWithBlockNumber},
        user_data::ProcessStatus,
    },
};
use intmax2_zkp::common::signature::key_set::KeySet;

use crate::external_api::contract::liquidity_contract::LiquidityContract;

use super::{common::fetch_decrypt_validate, error::StrategyError};

#[derive(Debug, Clone)]
pub struct DepositInfo {
    pub settled: Vec<(MetaDataWithBlockNumber, DepositData)>,
    pub pending: Vec<(MetaData, DepositData)>,
    pub timeout: Vec<(MetaData, DepositData)>,
}

#[allow(clippy::too_many_arguments)]
pub async fn fetch_deposit_info<S: StoreVaultClientInterface, V: ValidityProverClientInterface>(
    store_vault_server: &S,
    validity_prover: &V,
    liquidity_contract: &LiquidityContract,
    key: KeySet,
    included_uuids: &[String],
    excluded_uuids: &[String],
    cursor: &MetaDataCursor,
    deposit_timeout: u64,
) -> Result<(DepositInfo, MetaDataCursorResponse), StrategyError> {
    let mut settled = Vec::new();
    let mut pending = Vec::new();
    let mut timeout = Vec::new();
    let (data_with_meta, cursor_response) = fetch_decrypt_validate::<_, DepositData>(
        store_vault_server,
        key,
        DataType::Deposit,
        included_uuids,
        excluded_uuids,
        cursor,
    )
    .await?;
    for (meta, deposit_data) in data_with_meta {
        let token_index = liquidity_contract
            .get_token_index(
                deposit_data.token_type,
                deposit_data.token_address,
                deposit_data.token_id,
            )
            .await?;
        if token_index.is_none() {
            log::error!("Token not found: {:?}", deposit_data);
            // ignore this deposit
            continue;
        }
        let mut deposit_data = deposit_data;
        deposit_data.set_token_index(token_index.unwrap());
        let deposit_hash = deposit_data.deposit_hash().unwrap();
        if let Some(deposit_info) = validity_prover.get_deposit_info(deposit_hash).await? {
            let meta = MetaDataWithBlockNumber {
                meta,
                block_number: deposit_info.block_number,
            };
            settled.push((meta, deposit_data));
        } else if meta.timestamp + deposit_timeout < chrono::Utc::now().timestamp() as u64 {
            // timeout
            log::error!(
                "Deposit uuid: {}, hash: {} is timeout",
                meta.uuid,
                deposit_hash
            );
            timeout.push((meta, deposit_data));
        } else {
            // pending
            log::info!("Deposit {} is pending", meta.uuid);
            pending.push((meta, deposit_data));
        }
    }

    // sort
    settled.sort_by_key(|(meta, _)| (meta.block_number, meta.meta.uuid.clone()));
    pending.sort_by_key(|(meta, _)| (meta.timestamp, meta.uuid.clone()));
    timeout.sort_by_key(|(meta, _)| (meta.timestamp, meta.uuid.clone()));
    if cursor.order == CursorOrder::Desc {
        settled.reverse();
        pending.reverse();
        timeout.reverse();
    }

    Ok((
        DepositInfo {
            settled,
            pending,
            timeout,
        },
        cursor_response,
    ))
}

pub async fn fetch_all_unprocessed_deposit_info<
    S: StoreVaultClientInterface,
    V: ValidityProverClientInterface,
>(
    store_vault_server: &S,
    validity_prover: &V,
    liquidity_contract: &LiquidityContract,
    key: KeySet,
    process_status: &ProcessStatus,
    deposit_timeout: u64,
) -> Result<DepositInfo, StrategyError> {
    let mut cursor = MetaDataCursor {
        cursor: process_status.last_processed_meta_data.clone(),
        order: CursorOrder::Asc,
        limit: None,
    };
    let mut included_uuids = process_status.processed_uuids.clone(); // cleared after first fetch

    let mut settled = Vec::new();
    let mut pending = Vec::new();
    let mut timeout = Vec::new();
    loop {
        let (
            DepositInfo {
                settled: settled_part,
                pending: pending_part,
                timeout: timeout_part,
            },
            cursor_response,
        ) = fetch_deposit_info(
            store_vault_server,
            validity_prover,
            liquidity_contract,
            key,
            &included_uuids,
            &process_status.processed_uuids,
            &cursor,
            deposit_timeout,
        )
        .await?;
        if !included_uuids.is_empty() {
            included_uuids = Vec::new(); // clear included_uuids after first fetch
        }

        settled.extend(settled_part);
        pending.extend(pending_part);
        timeout.extend(timeout_part);
        if !cursor_response.has_more {
            break;
        }
        cursor.cursor = cursor_response.next_cursor;
    }

    Ok(DepositInfo {
        settled,
        pending,
        timeout,
    })
}
