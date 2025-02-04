use intmax2_zkp::ethereum_types::bytes32::Bytes32;

use crate::trees::incremental_merkle_tree::HistoricalIncrementalMerkleTree;

use super::merkle_tree::IncrementalMerkleTreeClient;

pub type HistoricalBlockHashTree = IncrementalMerkleTreeClient<Bytes32>;
