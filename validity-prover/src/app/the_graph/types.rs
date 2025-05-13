use intmax2_zkp::ethereum_types::{address::Address, bytes32::Bytes32};
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

    // metadata
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

// {
//     "depositHash": "0xd57a2c7e4431b8cec99930e570f380ba08ce341303cbbd99144078cc14150822",
//     "depositIndex": "5",
//     "transactionHash": "0x1c23bb0a7d0672f7eb34ca896d1be2ee3c39172e99786d7d930a4b4d2b8c97d0"
//   },
