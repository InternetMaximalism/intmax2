use intmax2_client_sdk::client::client::{DepositResult, TxResult};
use intmax2_interfaces::data::{
    deposit_data::DepositData,
    transfer_data::TransferData,
    tx_data::TxData,
    user_data::{Balances, UserData},
};
use intmax2_zkp::{common::transfer::Transfer, ethereum_types::u32limb_trait::U32LimbTrait as _};
use wasm_bindgen::prelude::wasm_bindgen;

use super::common::{JsTransfer, JsTx};

#[derive(Debug, Clone)]
#[wasm_bindgen(getter_with_clone)]
pub struct JsDepositData {
    pub deposit_salt: String,     // hex string
    pub depositor: String,        // hex string
    pub pubkey_salt_hash: String, // hex string
    pub amount: String,           // 10 base string
    pub is_eligible: bool,
    pub token_type: u8,
    pub token_address: String, // hex string
    pub token_id: String,      // 10 base string
    pub is_mining: bool,
    pub token_index: Option<u32>, // The index of the token in the contract
}

impl From<DepositData> for JsDepositData {
    fn from(deposit_data: DepositData) -> Self {
        Self {
            deposit_salt: deposit_data.deposit_salt.to_string(),
            depositor: deposit_data.depositor.to_hex(),
            pubkey_salt_hash: deposit_data.pubkey_salt_hash.to_hex(),
            amount: deposit_data.amount.to_string(),
            is_eligible: deposit_data.is_eligible,
            token_type: deposit_data.token_type as u8,
            token_address: deposit_data.token_address.to_hex(),
            token_id: deposit_data.token_id.to_string(),
            is_mining: deposit_data.is_mining,
            token_index: deposit_data.token_index,
        }
    }
}

#[derive(Debug, Clone)]
#[wasm_bindgen(getter_with_clone)]
pub struct JsTransferData {
    pub sender: String,
    pub transfer: JsTransfer,
}

impl From<TransferData> for JsTransferData {
    fn from(transfer_data: TransferData) -> Self {
        Self {
            sender: transfer_data.sender.to_hex(),
            transfer: transfer_data.transfer.into(),
        }
    }
}

#[derive(Debug, Clone)]
#[wasm_bindgen(getter_with_clone)]
pub struct JsTxData {
    pub tx: JsTx,
    pub transfers: Vec<JsTransfer>,
}

impl From<TxData> for JsTxData {
    fn from(tx_data: TxData) -> Self {
        let tx: JsTx = tx_data.spent_witness.tx.into();
        let transfers = tx_data
            .spent_witness
            .transfers
            .into_iter()
            .flat_map(|transfer| {
                if transfer == Transfer::default() {
                    // ignore default transfer
                    None
                } else {
                    Some(transfer.into())
                }
            })
            .collect();
        Self { tx, transfers }
    }
}

#[derive(Debug, Clone)]
#[wasm_bindgen(getter_with_clone)]
pub struct JsDepositResult {
    pub deposit_data: JsDepositData,
    pub deposit_digest: String,
    pub backup_csv: String,
}

impl From<DepositResult> for JsDepositResult {
    fn from(deposit_result: DepositResult) -> Self {
        Self {
            deposit_data: deposit_result.deposit_data.into(),
            deposit_digest: deposit_result.deposit_digest.to_string(),
            backup_csv: deposit_result.backup_csv,
        }
    }
}

#[derive(Debug, Clone)]
#[wasm_bindgen(getter_with_clone)]
pub struct JsTxResult {
    pub tx_tree_root: String,
    pub transfer_digests: Vec<String>,
    pub withdrawal_digests: Vec<String>,
    pub transfer_data_vec: Vec<JsTransferData>,
    pub withdrawal_data_vec: Vec<JsTransferData>,
    pub backup_csv: String,
}

impl From<TxResult> for JsTxResult {
    fn from(tx_result: TxResult) -> Self {
        Self {
            tx_tree_root: tx_result.tx_tree_root.to_hex(),
            transfer_digests: tx_result
                .transfer_digests
                .into_iter()
                .map(|x| x.to_hex())
                .collect(),
            withdrawal_digests: tx_result
                .withdrawal_digests
                .into_iter()
                .map(|x| x.to_hex())
                .collect(),
            transfer_data_vec: tx_result
                .transfer_data_vec
                .into_iter()
                .map(JsTransferData::from)
                .collect(),
            withdrawal_data_vec: tx_result
                .withdrawal_data_vec
                .into_iter()
                .map(JsTransferData::from)
                .collect(),
            backup_csv: tx_result.backup_csv,
        }
    }
}

#[derive(Debug, Clone)]
#[wasm_bindgen(getter_with_clone)]
pub struct JsUserData {
    /// The user public key
    pub pubkey: String,

    /// The token balances of the user
    pub balances: Vec<TokenBalance>,

    /// The private commitment of the user
    pub private_commitment: String,

    /// The last unix timestamp of processed deposits
    pub deposit_lpt: u64,

    /// The last unix timestamp of processed transfers
    pub transfer_lpt: u64,

    /// The last unix timestamp of processed txs
    pub tx_lpt: u64,

    /// The last unix timestamp of processed withdrawals
    pub withdrawal_lpt: u64,

    /// Digests of processed deposits
    pub processed_deposit_digests: Vec<String>,

    /// Digests of processed transfers
    pub processed_transfer_digests: Vec<String>,

    /// Digests of processed txs
    pub processed_tx_digests: Vec<String>,

    /// Digests of processed withdrawals
    pub processed_withdrawal_digests: Vec<String>,
}

#[derive(Debug, Clone)]
#[wasm_bindgen(getter_with_clone)]
pub struct TokenBalance {
    /// Token index of the balance
    pub token_index: u32,

    /// The amount of the token. 10 base string
    pub amount: String,

    /// Flag indicating whether the balance is insufficient for that token index.
    /// If true, subsequent transfers or withdrawals for that token index will be impossible.
    pub is_insufficient: bool,
}

impl From<UserData> for JsUserData {
    fn from(user_data: UserData) -> Self {
        Self {
            pubkey: user_data.pubkey.to_hex(),
            balances: balances_to_token_balances(&user_data.balances()),
            private_commitment: user_data
                .full_private_state
                .to_private_state()
                .commitment()
                .to_string(),
            deposit_lpt: user_data
                .deposit_status
                .last_processed_meta_data
                .as_ref()
                .map(|x| x.timestamp)
                .unwrap_or(0),
            transfer_lpt: user_data
                .transfer_status
                .last_processed_meta_data
                .as_ref()
                .map(|x| x.timestamp)
                .unwrap_or(0),
            tx_lpt: user_data
                .tx_status
                .last_processed_meta_data
                .as_ref()
                .map(|x| x.timestamp)
                .unwrap_or(0),
            withdrawal_lpt: user_data
                .withdrawal_status
                .last_processed_meta_data
                .as_ref()
                .map(|x| x.timestamp)
                .unwrap_or(0),
            processed_deposit_digests: user_data
                .deposit_status
                .processed_digests
                .into_iter()
                .map(|x| x.to_hex())
                .collect(),
            processed_transfer_digests: user_data
                .transfer_status
                .processed_digests
                .into_iter()
                .map(|x| x.to_hex())
                .collect(),
            processed_tx_digests: user_data
                .tx_status
                .processed_digests
                .into_iter()
                .map(|x| x.to_hex())
                .collect(),
            processed_withdrawal_digests: user_data
                .withdrawal_status
                .processed_digests
                .into_iter()
                .map(|x| x.to_hex())
                .collect(),
        }
    }
}

pub fn balances_to_token_balances(balances: &Balances) -> Vec<TokenBalance> {
    balances
        .0
        .iter()
        .map(|(token_index, leaf)| {
            let amount = leaf.amount.to_string();
            let is_insufficient = leaf.is_insufficient;
            TokenBalance {
                token_index: *token_index,
                amount,
                is_insufficient,
            }
        })
        .collect()
}
