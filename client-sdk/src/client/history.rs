use intmax2_interfaces::{
    api::{
        balance_prover::interface::BalanceProverClientInterface,
        block_builder::interface::BlockBuilderClientInterface,
        store_vault_server::interface::StoreVaultClientInterface,
        validity_prover::interface::ValidityProverClientInterface,
        withdrawal_server::interface::WithdrawalServerClientInterface,
    },
    data::{
        deposit_data::DepositData,
        meta_data::{MetaData, MetaDataWithBlockNumber},
        transfer_data::TransferData,
        tx_data::TxData,
        user_data::ProcessStatus,
    },
};
use intmax2_zkp::common::signature::key_set::KeySet;
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
    pub fn from_settled(processed_uuids: &[String], meta: MetaDataWithBlockNumber) -> Self {
        if processed_uuids.contains(&meta.meta.uuid) {
            EntryStatus::Processed(meta.block_number)
        } else {
            EntryStatus::Settled(meta.block_number)
        }
    }
}

pub async fn fetch_deposit_history<
    BB: BlockBuilderClientInterface,
    S: StoreVaultClientInterface,
    V: ValidityProverClientInterface,
    B: BalanceProverClientInterface,
    W: WithdrawalServerClientInterface,
>(
    client: &Client<BB, S, V, B, W>,
    key: KeySet,
) -> Result<Vec<HistoryEntry<DepositData>>, ClientError> {
    let user_data = client.get_user_data(key).await?;

    let mut history = Vec::new();
    let all_deposit_info = fetch_deposit_info(
        &client.store_vault_server,
        &client.validity_prover,
        &client.liquidity_contract,
        key,
        &ProcessStatus::default(),
        client.config.deposit_timeout,
    )
    .await?;
    for (meta, settled) in all_deposit_info.settled {
        history.push(HistoryEntry {
            data: settled,
            status: EntryStatus::from_settled(
                &user_data.deposit_status.processed_uuids,
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
        (meta.timestamp, meta.uuid.clone())
    });

    Ok(history)
}

pub async fn fetch_transfer_history<
    BB: BlockBuilderClientInterface,
    S: StoreVaultClientInterface,
    V: ValidityProverClientInterface,
    B: BalanceProverClientInterface,
    W: WithdrawalServerClientInterface,
>(
    client: &Client<BB, S, V, B, W>,
    key: KeySet,
) -> Result<Vec<HistoryEntry<TransferData>>, ClientError> {
    let user_data = client.get_user_data(key).await?;

    let mut history = Vec::new();
    let all_transfers_info = fetch_transfer_info(
        &client.store_vault_server,
        &client.validity_prover,
        key,
        &ProcessStatus::default(),
        client.config.tx_timeout,
    )
    .await?;
    for (meta, settled) in all_transfers_info.settled {
        history.push(HistoryEntry {
            data: settled,
            status: EntryStatus::from_settled(
                &user_data.transfer_status.processed_uuids,
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
        (meta.timestamp, meta.uuid.clone())
    });

    Ok(history)
}

pub async fn fetch_tx_history<
    BB: BlockBuilderClientInterface,
    S: StoreVaultClientInterface,
    V: ValidityProverClientInterface,
    B: BalanceProverClientInterface,
    W: WithdrawalServerClientInterface,
>(
    client: &Client<BB, S, V, B, W>,
    key: KeySet,
) -> Result<Vec<HistoryEntry<TxData>>, ClientError> {
    let user_data = client.get_user_data(key).await?;

    let mut history = Vec::new();
    let all_tx_info = fetch_tx_info(
        &client.store_vault_server,
        &client.validity_prover,
        key,
        &ProcessStatus::default(),
        client.config.tx_timeout,
    )
    .await?;
    for (meta, settled) in all_tx_info.settled {
        history.push(HistoryEntry {
            data: settled,
            status: EntryStatus::from_settled(&user_data.tx_status.processed_uuids, meta.clone()),
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
        (meta.timestamp, meta.uuid.clone())
    });

    Ok(history)
}
