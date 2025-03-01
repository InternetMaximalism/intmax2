use async_trait::async_trait;
use intmax2_zkp::{
    circuits::validity::validity_pis::ValidityPublicInputs,
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

use crate::api::error::ServerError;

type F = GoldilocksField;
type C = PoseidonGoldilocksConfig;
const D: usize = 2;

pub const MAX_BATCH_SIZE: usize = 128;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DepositInfo {
    pub deposit_hash: Bytes32,
    pub block_number: u32,
    pub deposit_index: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountInfo {
    pub account_id: Option<u64>,
    pub block_number: u32,
    pub last_block_number: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransitionProofTask {
    pub block_number: u32,
    pub prev_validity_pis: ValidityPublicInputs,
    pub validity_witness: ValidityWitness,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransitionProofTaskResult {
    pub block_number: u32,
    pub proof: Option<ProofWithPublicInputs<F, C, D>>,
    pub error: Option<String>,
}

#[async_trait(?Send)]
pub trait ValidityProverClientInterface {
    async fn get_block_number(&self) -> Result<u32, ServerError>;

    async fn get_validity_proof_block_number(&self) -> Result<u32, ServerError>;

    async fn get_next_deposit_index(&self) -> Result<u32, ServerError>;

    async fn get_update_witness(
        &self,
        pubkey: U256,
        root_block_number: u32,
        leaf_block_number: u32,
        is_prev_account_tree: bool,
    ) -> Result<UpdateWitness<F, C, D>, ServerError>;

    async fn get_deposit_info(
        &self,
        deposit_hash: Bytes32,
    ) -> Result<Option<DepositInfo>, ServerError>;

    async fn get_deposit_info_batch(
        &self,
        deposit_hashes: &[Bytes32],
    ) -> Result<Vec<Option<DepositInfo>>, ServerError>;

    async fn get_block_number_by_tx_tree_root(
        &self,
        tx_tree_root: Bytes32,
    ) -> Result<Option<u32>, ServerError>;

    async fn get_block_number_by_tx_tree_root_batch(
        &self,
        tx_tree_roots: &[Bytes32],
    ) -> Result<Vec<Option<u32>>, ServerError>;

    async fn get_validity_witness(&self, block_number: u32)
        -> Result<ValidityWitness, ServerError>;

    async fn get_block_merkle_proof(
        &self,
        root_block_number: u32,
        leaf_block_number: u32,
    ) -> Result<BlockHashMerkleProof, ServerError>;

    async fn get_deposit_merkle_proof(
        &self,
        block_number: u32,
        deposit_index: u32,
    ) -> Result<DepositMerkleProof, ServerError>;

    async fn get_account_info(&self, pubkey: U256) -> Result<AccountInfo, ServerError>;

    async fn get_account_info_batch(
        &self,
        pubkeys: &[U256],
    ) -> Result<Vec<AccountInfo>, ServerError>;
}
