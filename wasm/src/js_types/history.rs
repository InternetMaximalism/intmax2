use intmax2_client_sdk::client::history::{EntryStatus, HistoryEntry};
use intmax2_interfaces::data::{
    deposit_data::DepositData, transfer_data::TransferData, tx_data::TxData,
};
use wasm_bindgen::prelude::wasm_bindgen;

use super::{
    common::JsMetaData,
    data::{JsDepositData, JsTransferData, JsTxData},
};

#[derive(Debug, Clone)]
#[wasm_bindgen(getter_with_clone)]
pub struct JsEntryStatusWithBlockNumber {
    /// The status of the entry
    /// - "settled": The entry has been on-chain but not yet incorporated into the proof
    /// - "processed": The entry has been incorporated into the proof
    /// - "pending": The entry is not yet on-chain
    /// - "timeout": The entry is not yet on-chain and has timed out
    pub status: String,
    pub block_number: Option<u32>,
}

impl From<EntryStatus> for JsEntryStatusWithBlockNumber {
    fn from(status: EntryStatus) -> Self {
        match status {
            EntryStatus::Settled(b) => Self {
                status: "settled".to_string(),
                block_number: Some(b),
            },
            EntryStatus::Processed(b) => Self {
                status: "processed".to_string(),
                block_number: Some(b),
            },
            EntryStatus::Pending => Self {
                status: "pending".to_string(),
                block_number: None,
            },
            EntryStatus::Timeout => Self {
                status: "timeout".to_string(),
                block_number: None,
            },
        }
    }
}
#[derive(Clone, Debug)]
#[wasm_bindgen(getter_with_clone)]
pub struct JsDepositEntry {
    pub data: JsDepositData,
    pub status: JsEntryStatusWithBlockNumber,
    pub meta: JsMetaData,
}

#[derive(Clone, Debug)]
#[wasm_bindgen(getter_with_clone)]
pub struct JsTransferEntry {
    pub data: JsTransferData,
    pub status: JsEntryStatusWithBlockNumber,
    pub meta: JsMetaData,
}

#[derive(Clone, Debug)]
#[wasm_bindgen(getter_with_clone)]
pub struct JsTxEntry {
    pub data: JsTxData,
    pub status: JsEntryStatusWithBlockNumber,
    pub meta: JsMetaData,
}

impl From<HistoryEntry<DepositData>> for JsDepositEntry {
    fn from(entry: HistoryEntry<DepositData>) -> Self {
        Self {
            data: entry.data.into(),
            status: entry.status.into(),
            meta: entry.meta.into(),
        }
    }
}

impl From<HistoryEntry<TransferData>> for JsTransferEntry {
    fn from(entry: HistoryEntry<TransferData>) -> Self {
        Self {
            data: entry.data.into(),
            status: entry.status.into(),
            meta: entry.meta.into(),
        }
    }
}

impl From<HistoryEntry<TxData>> for JsTxEntry {
    fn from(entry: HistoryEntry<TxData>) -> Self {
        Self {
            data: entry.data.into(),
            status: entry.status.into(),
            meta: entry.meta.into(),
        }
    }
}
