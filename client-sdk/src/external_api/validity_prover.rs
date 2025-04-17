use async_trait::async_trait;
use intmax2_interfaces::api::{
    error::ServerError,
    validity_prover::{
        interface::{AccountInfo, DepositInfo, ValidityProverClientInterface, MAX_BATCH_SIZE},
        types::{
            GetAccountInfoBatchRequest, GetAccountInfoBatchResponse, GetAccountInfoQuery,
            GetAccountInfoResponse, GetBlockMerkleProofQuery, GetBlockMerkleProofResponse,
            GetBlockNumberByTxTreeRootBatchRequest, GetBlockNumberByTxTreeRootBatchResponse,
            GetBlockNumberByTxTreeRootQuery, GetBlockNumberByTxTreeRootResponse,
            GetBlockNumberResponse, GetDepositInfoBatchRequest, GetDepositInfoBatchResponse,
            GetDepositInfoQuery, GetDepositInfoResponse, GetDepositMerkleProofQuery,
            GetDepositMerkleProofResponse, GetLatestIncludedDepositIndexResponse,
            GetNextDepositIndexResponse, GetUpdateWitnessQuery, GetUpdateWitnessResponse,
            GetValidityWitnessQuery, GetValidityWitnessResponse,
        },
    },
};
use intmax2_zkp::{
    common::{
        trees::{block_hash_tree::BlockHashMerkleProof, deposit_tree::DepositMerkleProof},
        witness::{update_witness::UpdateWitness, validity_witness::ValidityWitness},
    },
    ethereum_types::{bytes32::Bytes32, u256::U256},
};
use plonky2::{field::goldilocks_field::GoldilocksField, plonk::config::PoseidonGoldilocksConfig};

use super::utils::query::{get_request, post_request};

type F = GoldilocksField;
type C = PoseidonGoldilocksConfig;
const D: usize = 2;

#[derive(Debug, Clone)]
pub struct ValidityProverClient {
    base_url: String,
}

impl ValidityProverClient {
    pub fn new(base_url: &str) -> Self {
        ValidityProverClient {
            base_url: base_url.to_string(),
        }
    }

    pub async fn sync(&self) -> Result<(), ServerError> {
        get_request::<(), ()>(&self.base_url, "/validity-prover/sync", None).await?;
        Ok(())
    }
}

#[async_trait(?Send)]
impl ValidityProverClientInterface for ValidityProverClient {
    async fn get_block_number(&self) -> Result<u32, ServerError> {
        let response: GetBlockNumberResponse =
            get_request::<(), _>(&self.base_url, "/validity-prover/block-number", None).await?;
        Ok(response.block_number)
    }

    async fn get_validity_proof_block_number(&self) -> Result<u32, ServerError> {
        let response: GetBlockNumberResponse = get_request::<(), _>(
            &self.base_url,
            "/validity-prover/validity-proof-block-number",
            None,
        )
        .await?;
        Ok(response.block_number)
    }

    async fn get_next_deposit_index(&self) -> Result<u32, ServerError> {
        let response: GetNextDepositIndexResponse =
            get_request::<(), _>(&self.base_url, "/validity-prover/next-deposit-index", None)
                .await?;
        Ok(response.deposit_index)
    }

    async fn get_latest_included_deposit_index(&self) -> Result<Option<u32>, ServerError> {
        let response: GetLatestIncludedDepositIndexResponse = get_request::<(), _>(
            &self.base_url,
            "/validity-prover/latest-included-deposit-index",
            None,
        )
        .await?;
        Ok(response.deposit_index)
    }

    async fn get_update_witness(
        &self,
        pubkey: U256,
        root_block_number: u32,
        leaf_block_number: u32,
        is_prev_account_tree: bool,
    ) -> Result<UpdateWitness<F, C, D>, ServerError> {
        let query = GetUpdateWitnessQuery {
            pubkey,
            root_block_number,
            leaf_block_number,
            is_prev_account_tree,
        };
        let response: GetUpdateWitnessResponse = get_request(
            &self.base_url,
            "/validity-prover/get-update-witness",
            Some(query),
        )
        .await?;
        Ok(response.update_witness)
    }

    async fn get_deposit_info(
        &self,
        pubkey_salt_hash: Bytes32,
    ) -> Result<Option<DepositInfo>, ServerError> {
        let query = GetDepositInfoQuery { pubkey_salt_hash };
        let response: GetDepositInfoResponse = get_request(
            &self.base_url,
            "/validity-prover/get-deposit-info",
            Some(query),
        )
        .await?;
        Ok(response.deposit_info)
    }

    async fn get_deposit_info_batch(
        &self,
        pubkey_salt_hashes: &[Bytes32],
    ) -> Result<Vec<Option<DepositInfo>>, ServerError> {
        let mut all_deposit_info = Vec::with_capacity(pubkey_salt_hashes.len());

        for chunk in pubkey_salt_hashes.chunks(MAX_BATCH_SIZE) {
            let request = GetDepositInfoBatchRequest {
                pubkey_salt_hashes: chunk.to_vec(),
            };

            let response: GetDepositInfoBatchResponse = post_request(
                &self.base_url,
                "/validity-prover/get-deposit-info-batch",
                Some(&request),
            )
            .await?;

            all_deposit_info.extend(response.deposit_info);
        }

        Ok(all_deposit_info)
    }

    async fn get_block_number_by_tx_tree_root(
        &self,
        tx_tree_root: Bytes32,
    ) -> Result<Option<u32>, ServerError> {
        let query = GetBlockNumberByTxTreeRootQuery { tx_tree_root };
        let response: GetBlockNumberByTxTreeRootResponse = get_request(
            &self.base_url,
            "/validity-prover/get-block-number-by-tx-tree-root",
            Some(query),
        )
        .await?;
        Ok(response.block_number)
    }

    async fn get_block_number_by_tx_tree_root_batch(
        &self,
        tx_tree_roots: &[Bytes32],
    ) -> Result<Vec<Option<u32>>, ServerError> {
        let mut all_block_numbers = Vec::with_capacity(tx_tree_roots.len());

        for chunk in tx_tree_roots.chunks(MAX_BATCH_SIZE) {
            let request = GetBlockNumberByTxTreeRootBatchRequest {
                tx_tree_roots: chunk.to_vec(),
            };
            let response: GetBlockNumberByTxTreeRootBatchResponse = post_request(
                &self.base_url,
                "/validity-prover/get-block-number-by-tx-tree-root-batch",
                Some(&request),
            )
            .await?;
            all_block_numbers.extend(response.block_numbers);
        }

        Ok(all_block_numbers)
    }

    async fn get_validity_witness(
        &self,
        block_number: u32,
    ) -> Result<ValidityWitness, ServerError> {
        let query = GetValidityWitnessQuery { block_number };
        let response: GetValidityWitnessResponse = get_request(
            &self.base_url,
            "/validity-prover/get-validity-witness",
            Some(query),
        )
        .await?;
        Ok(response.validity_witness)
    }

    async fn get_block_merkle_proof(
        &self,
        root_block_number: u32,
        leaf_block_number: u32,
    ) -> Result<BlockHashMerkleProof, ServerError> {
        let query = GetBlockMerkleProofQuery {
            root_block_number,
            leaf_block_number,
        };
        let response: GetBlockMerkleProofResponse = get_request(
            &self.base_url,
            "/validity-prover/get-block-merkle-proof",
            Some(query),
        )
        .await?;
        Ok(response.block_merkle_proof)
    }

    async fn get_deposit_merkle_proof(
        &self,
        block_number: u32,
        deposit_index: u32,
    ) -> Result<DepositMerkleProof, ServerError> {
        let query = GetDepositMerkleProofQuery {
            block_number,
            deposit_index,
        };
        let response: GetDepositMerkleProofResponse = get_request(
            &self.base_url,
            "/validity-prover/get-deposit-merkle-proof",
            Some(query),
        )
        .await?;
        Ok(response.deposit_merkle_proof)
    }

    async fn get_account_info(&self, pubkey: U256) -> Result<AccountInfo, ServerError> {
        let query = GetAccountInfoQuery { pubkey };
        let response: GetAccountInfoResponse = get_request(
            &self.base_url,
            "/validity-prover/get-account-info",
            Some(query),
        )
        .await?;
        Ok(response.account_info)
    }

    async fn get_account_info_batch(
        &self,
        pubkeys: &[U256],
    ) -> Result<Vec<AccountInfo>, ServerError> {
        let mut all_account_info = Vec::with_capacity(pubkeys.len());

        for chunk in pubkeys.chunks(MAX_BATCH_SIZE) {
            let request = GetAccountInfoBatchRequest {
                pubkeys: chunk.to_vec(),
            };
            let response: GetAccountInfoBatchResponse = post_request(
                &self.base_url,
                "/validity-prover/get-account-info-batch",
                Some(&request),
            )
            .await?;
            all_account_info.extend(response.account_info);
        }

        Ok(all_account_info)
    }
}
