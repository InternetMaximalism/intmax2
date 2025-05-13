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
    pub id: String,
    pub prev_block_hash: Bytes32,
    pub block_builder: Address,
    #[serde_as(as = "DisplayFromStr")]
    pub rollup_block_number: u32,
    pub deposit_tree_root: Bytes32,
    #[serde_as(as = "DisplayFromStr")]
    pub timestamp: u64,
    pub transaction_hash: Bytes32,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BlockPostedsData {
    pub block_posteds: Vec<BlockPostedEntry>,
}
