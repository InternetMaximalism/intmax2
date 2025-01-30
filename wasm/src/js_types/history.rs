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
#[wasm_bindgen]
pub enum JsEntryStatus {
    Settled,   // Settled at block number but not processed yet
    Processed, // Incorporated into the balance proof
    Pending,   // Not settled yet
    Timeout,   // Timed out
}

#[derive(Debug, Clone)]
#[wasm_bindgen(getter_with_clone)]
pub struct JsEntryStatusWithBlockNumber {
    pub status: JsEntryStatus,
    pub block_number: Option<u32>,
}

impl From<EntryStatus> for JsEntryStatusWithBlockNumber {
    fn from(status: EntryStatus) -> Self {
        match status {
            EntryStatus::Settled(b) => Self {
                status: JsEntryStatus::Settled,
                block_number: Some(b),
            },
            EntryStatus::Processed(b) => Self {
                status: JsEntryStatus::Processed,
                block_number: Some(b),
            },
            EntryStatus::Pending => Self {
                status: JsEntryStatus::Pending,
                block_number: None,
            },
            EntryStatus::Timeout => Self {
                status: JsEntryStatus::Timeout,
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
