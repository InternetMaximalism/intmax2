use intmax2_interfaces::data::meta_data::{MetaData, MetaDataWithBlockNumber};
use intmax2_zkp::ethereum_types::bytes32::Bytes32;
use serde::{Deserialize, Serialize};

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

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry<T> {
    pub data: T,
    pub status: EntryStatus,
    pub meta: MetaData,
}
