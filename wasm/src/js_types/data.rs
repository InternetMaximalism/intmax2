use intmax2_interfaces::data::{
    deposit_data::DepositData,
    extra_data::{ExtraData, FullExtraData},
    meta_data::MetaData,
    transfer_data::TransferData,
    tx_data::TxData,
    user_data::{Balances, UserData},
};
use intmax2_zkp::{
    common::transfer::Transfer,
    ethereum_types::{bytes32::Bytes32, u32limb_trait::U32LimbTrait as _},
};
use wasm_bindgen::prelude::wasm_bindgen;

use crate::js_types::keys::JsPublicKeyPair;

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
    pub sender: JsPublicKeyPair,
    pub transfer: JsTransfer,
    pub extra_data: JsExtraData,
}

impl From<TransferData> for JsTransferData {
    fn from(transfer_data: TransferData) -> Self {
        Self {
            sender: transfer_data.sender.into(),
            transfer: transfer_data.transfer.into(),
            extra_data: JsExtraData::from(transfer_data.extra_data),
        }
    }
}

#[derive(Debug, Clone)]
#[wasm_bindgen(getter_with_clone)]
pub struct JsTxData {
    pub tx: JsTx,
    pub tx_index: u32,
    pub tx_tree_root: String,
    pub transfers: Vec<JsTransfer>,
    pub transfer_digests: Vec<String>,
    pub transfer_types: Vec<String>,
    pub recipient_view_pubs: Vec<String>,
    pub full_extra_data: Vec<JsFullExtraData>,
}

impl From<TxData> for JsTxData {
    fn from(tx_data: TxData) -> Self {
        let tx: JsTx = tx_data.spent_witness.tx.into();
        let transfers = tx_data
            .spent_witness
            .transfers
            .into_iter()
            .filter(|t| *t != Transfer::default())
            .map(Into::into)
            .collect();
        let transfer_digests = tx_data
            .transfer_digests
            .into_iter()
            .map(|digest| digest.to_hex())
            .collect();
        let recipient_view_pubs = tx_data
            .recipient_view_pubs
            .into_iter()
            .map(|pubkey| pubkey.to_string())
            .collect();
        let full_extra_data = tx_data
            .full_extra_data
            .into_iter()
            .map(JsFullExtraData::from)
            .collect();
        Self {
            tx,
            tx_index: tx_data.tx_index,
            tx_tree_root: tx_data.tx_tree_root.to_hex(),
            transfers,
            transfer_digests,
            transfer_types: tx_data.transfer_types,
            recipient_view_pubs,
            full_extra_data,
        }
    }
}

#[derive(Debug, Clone)]
#[wasm_bindgen(getter_with_clone)]
pub struct JsFullExtraData {
    pub payment_id: Option<String>,
    pub description: Option<String>,
    pub description_salt: Option<String>,
    pub inner_salt: Option<String>,
}

impl From<FullExtraData> for JsFullExtraData {
    fn from(full_extra_data: FullExtraData) -> Self {
        Self {
            payment_id: full_extra_data.payment_id.map(|id| id.to_hex()),
            description: full_extra_data.description,
            description_salt: full_extra_data.description_salt.map(|salt| salt.to_hex()),
            inner_salt: full_extra_data.inner_salt.map(|salt| salt.to_hex()),
        }
    }
}

#[derive(Debug, Clone)]
#[wasm_bindgen(getter_with_clone)]
pub struct JsExtraData {
    pub payment_id: Option<String>,
    pub description_hash: Option<String>,
    pub inner_salt: Option<String>,
}

impl From<ExtraData> for JsExtraData {
    fn from(extra_data: ExtraData) -> Self {
        Self {
            payment_id: extra_data.payment_id.map(|id| id.to_hex()),
            description_hash: extra_data.description_hash.map(|hash| hash.to_hex()),
            inner_salt: extra_data.inner_salt.map(|salt| salt.to_hex()),
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

fn extract_timestamp(opt: &Option<MetaData>) -> u64 {
    opt.as_ref().map(|x| x.timestamp).unwrap_or(0)
}

fn convert_bytes32_vec_to_hex(digests: Vec<Bytes32>) -> Vec<String> {
    digests.into_iter().map(|x| x.to_hex()).collect()
}

impl From<UserData> for JsUserData {
    fn from(user_data: UserData) -> Self {
        Self {
            pubkey: user_data.pubkey.to_hex(),
            balances: balances_to_token_balances(user_data.balances()),
            private_commitment: user_data
                .full_private_state
                .to_private_state()
                .commitment()
                .to_string(),
            deposit_lpt: extract_timestamp(&user_data.deposit_status.last_processed_meta_data),
            transfer_lpt: extract_timestamp(&user_data.transfer_status.last_processed_meta_data),
            tx_lpt: extract_timestamp(&user_data.tx_status.last_processed_meta_data),
            withdrawal_lpt: extract_timestamp(
                &user_data.withdrawal_status.last_processed_meta_data,
            ),
            processed_deposit_digests: convert_bytes32_vec_to_hex(
                user_data.deposit_status.processed_digests,
            ),
            processed_transfer_digests: convert_bytes32_vec_to_hex(
                user_data.transfer_status.processed_digests,
            ),
            processed_tx_digests: convert_bytes32_vec_to_hex(user_data.tx_status.processed_digests),
            processed_withdrawal_digests: convert_bytes32_vec_to_hex(
                user_data.withdrawal_status.processed_digests,
            ),
        }
    }
}

pub fn balances_to_token_balances(balances: Balances) -> Vec<TokenBalance> {
    balances
        .0
        .iter()
        .map(|(token_index, leaf)| TokenBalance {
            token_index: *token_index,
            amount: leaf.amount.to_string(),
            is_insufficient: leaf.is_insufficient,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use hashbrown::HashMap;
    use intmax2_interfaces::data::user_data::Balances;
    use intmax2_zkp::{common::trees::asset_tree::AssetLeaf, ethereum_types::u256::U256};

    #[test]
    fn test_balances_to_token_balances_basic() {
        let mut map = HashMap::new();
        map.insert(
            0,
            AssetLeaf {
                amount: U256::from(1000),
                is_insufficient: false,
            },
        );
        map.insert(
            1,
            AssetLeaf {
                amount: U256::from(0),
                is_insufficient: true,
            },
        );

        let balances = Balances(map);
        let mut result = balances_to_token_balances(balances);

        // Ensure consistent order for test (HashMap is unordered)
        result.sort_by_key(|b| b.token_index);

        assert_eq!(result.len(), 2);

        let b0 = &result[0];
        assert_eq!(b0.token_index, 0);
        assert_eq!(b0.amount, "1000");
        assert!(!b0.is_insufficient);

        let b1 = &result[1];
        assert_eq!(b1.token_index, 1);
        assert_eq!(b1.amount, "0");
        assert!(b1.is_insufficient);
    }

    #[test]
    fn test_balances_to_token_balances_empty() {
        let balances = Balances(HashMap::new());
        let result = balances_to_token_balances(balances);
        assert!(result.is_empty());
    }

    #[test]
    fn test_balances_to_token_balance_max_value() {
        let mut map = HashMap::new();
        map.insert(
            42,
            AssetLeaf {
                amount: U256::from_u32_slice(&[0xFFFFFFFF; 8]).unwrap(),
                is_insufficient: false,
            },
        );

        let balances = Balances(map);
        let result = balances_to_token_balances(balances);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].token_index, 42);
        assert_eq!(
            result[0].amount,
            U256::from_u32_slice(&[0xFFFFFFFF; 8]).unwrap().to_string()
        );
        assert!(!result[0].is_insufficient);
    }
}
