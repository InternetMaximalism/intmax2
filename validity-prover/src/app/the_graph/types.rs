use intmax2_zkp::ethereum_types::{address::Address, bytes32::Bytes32, u256::U256};
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphQLResponse<T> {
    pub data: T,
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BlockPostedEntry {
    pub prev_block_hash: Bytes32,
    pub block_builder: Address,
    pub deposit_tree_root: Bytes32,
    #[serde_as(as = "DisplayFromStr")]
    pub rollup_block_number: u32,
    #[serde_as(as = "DisplayFromStr")]
    pub block_timestamp: u64,
    pub transaction_hash: Bytes32,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BlockPostedsData {
    pub block_posteds: Vec<BlockPostedEntry>,
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DepositLeafInsertedEntry {
    pub deposit_hash: Bytes32,
    #[serde_as(as = "DisplayFromStr")]
    pub deposit_index: u32,
    pub transaction_hash: Bytes32,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DepositLeafInsertedData {
    pub deposit_leaf_inserteds: Vec<DepositLeafInsertedEntry>,
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DepositedEntry {
    #[serde_as(as = "DisplayFromStr")]
    pub deposit_id: u64,
    pub sender: Address,
    #[serde_as(as = "DisplayFromStr")]
    pub token_index: u32,
    pub amount: U256,
    pub recipient_salt_hash: Bytes32,
    pub is_eligible: bool,
    #[serde_as(as = "DisplayFromStr")]
    pub deposited_at: u64,
    pub transaction_hash: Bytes32,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DepositedData {
    pub depositeds: Vec<DepositedEntry>,
}
