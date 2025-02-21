use super::interface::{DepositInfo, TransitionProofTask};
use crate::api::validity_prover::interface::AccountInfo;
use intmax2_zkp::{
    common::{
        trees::{block_hash_tree::BlockHashMerkleProof, deposit_tree::DepositMerkleProof},
        witness::{update_witness::UpdateWitness, validity_witness::ValidityWitness},
    },
    ethereum_types::{bytes32::Bytes32, u256::U256},
};
use plonky2::{
    field::goldilocks_field::GoldilocksField,
    plonk::{config::PoseidonGoldilocksConfig, proof::ProofWithPublicInputs},
};
use serde::{Deserialize, Serialize};
type F = GoldilocksField;
type C = PoseidonGoldilocksConfig;
const D: usize = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetBlockNumberResponse {
    pub block_number: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetNextDepositIndexResponse {
    pub deposit_index: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetUpdateWitnessQuery {
    pub pubkey: U256,
    pub root_block_number: u32,
    pub leaf_block_number: u32,
    pub is_prev_account_tree: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetUpdateWitnessResponse {
    pub update_witness: UpdateWitness<F, C, D>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetDepositInfoQuery {
    pub deposit_hash: Bytes32,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetDepositInfoResponse {
    pub deposit_info: Option<DepositInfo>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetDepositInfoBatchRequest {
    pub deposit_hashes: Vec<Bytes32>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetDepositInfoBatchResponse {
    pub deposit_info: Vec<Option<DepositInfo>>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetBlockNumberByTxTreeRootQuery {
    pub tx_tree_root: Bytes32,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetBlockNumberByTxTreeRootResponse {
    pub block_number: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetBlockNumberByTxTreeRootBatchRequest {
    pub tx_tree_roots: Vec<Bytes32>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetBlockNumberByTxTreeRootBatchResponse {
    pub block_numbers: Vec<Option<u32>>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetValidityWitnessQuery {
    pub block_number: u32,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetValidityWitnessResponse {
    pub validity_witness: ValidityWitness,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetValidityPisQuery {
    pub block_number: u32,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetBlockMerkleProofQuery {
    pub root_block_number: u32,
    pub leaf_block_number: u32,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetBlockMerkleProofResponse {
    pub block_merkle_proof: BlockHashMerkleProof,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetDepositMerkleProofQuery {
    pub block_number: u32,
    pub deposit_index: u32,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetDepositMerkleProofResponse {
    pub deposit_merkle_proof: DepositMerkleProof,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetAccountInfoQuery {
    pub pubkey: U256,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetAccountInfoResponse {
    pub account_info: AccountInfo,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetAccountInfoBatchRequest {
    pub pubkeys: Vec<U256>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetAccountInfoBatchResponse {
    pub account_info: Vec<AccountInfo>,
}

// Below are Coordinator API
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssignResponse {
    pub task: Option<TransitionProofTask>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompleteRequest {
    pub block_number: u32,
    pub transition_proof: ProofWithPublicInputs<F, C, D>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeartBeatRequest {
    pub block_number: u32,
}
