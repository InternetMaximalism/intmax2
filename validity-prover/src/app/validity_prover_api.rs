use std::collections::HashMap;

use intmax2_interfaces::api::validity_prover::interface::AccountInfo;
use intmax2_zkp::{
    common::{
        trees::{
            account_tree::AccountMembershipProof, block_hash_tree::BlockHashMerkleProof,
            deposit_tree::DepositMerkleProof,
        },
        witness::{update_witness::UpdateWitness, validity_witness::ValidityWitness},
    },
    ethereum_types::{bytes32::Bytes32, u256::U256, u32limb_trait::U32LimbTrait as _},
    utils::trees::{incremental_merkle_tree::IncrementalMerkleProof, merkle_tree::MerkleProof},
};

use crate::trees::merkle_tree::IncrementalMerkleTreeClient;

use plonky2::{
    field::goldilocks_field::GoldilocksField,
    plonk::{config::PoseidonGoldilocksConfig, proof::ProofWithPublicInputs},
};

use super::{error::ValidityProverError, validity_prover::ValidityProver};
use crate::trees::merkle_tree::IndexedMerkleTreeClient;

type F = GoldilocksField;
type C = PoseidonGoldilocksConfig;
const D: usize = 2;

impl ValidityProver {
    pub async fn get_validity_proof(
        &self,
        block_number: u32,
    ) -> Result<Option<ProofWithPublicInputs<F, C, D>>, ValidityProverError> {
        let record = sqlx::query!(
            "SELECT proof FROM validity_proofs WHERE block_number = $1",
            block_number as i32
        )
        .fetch_optional(&self.pool)
        .await?;
        match record {
            Some(r) => {
                let proof: ProofWithPublicInputs<F, C, D> = bincode::deserialize(&r.proof)?;
                Ok(Some(proof))
            }
            None => Ok(None),
        }
    }

    pub async fn get_update_witness(
        &self,
        pubkey: U256,
        root_block_number: u32,
        leaf_block_number: u32,
        is_prev_account_tree: bool,
    ) -> Result<UpdateWitness<F, C, D>, ValidityProverError> {
        let validity_proof = self.get_validity_proof(root_block_number).await?.ok_or(
            ValidityProverError::ValidityProofNotFound(root_block_number),
        )?;

        let block_merkle_proof = self
            .get_block_merkle_proof(root_block_number, leaf_block_number)
            .await?;

        let account_tree_block_number = if is_prev_account_tree {
            root_block_number - 1
        } else {
            root_block_number
        };

        let account_membership_proof = self
            .get_account_membership_proof(account_tree_block_number, pubkey)
            .await?;

        Ok(UpdateWitness {
            is_prev_account_tree,
            validity_proof,
            block_merkle_proof,
            account_membership_proof,
        })
    }

    pub async fn get_account_info(&self, pubkey: U256) -> Result<AccountInfo, ValidityProverError> {
        let block_number = self.get_last_block_number().await?;
        let account_id = self.account_tree.index(block_number as u64, pubkey).await?;
        let last_block_number = if let Some(index) = account_id {
            let account_leaf = self
                .account_tree
                .get_leaf(block_number as u64, index)
                .await?;
            account_leaf.value as u32
        } else {
            0
        };
        Ok(AccountInfo {
            block_number,
            account_id,
            last_block_number,
        })
    }

    pub async fn get_account_info_batch(
        &self,
        pubkeys: &[U256],
    ) -> Result<Vec<AccountInfo>, ValidityProverError> {
        // early return for empty input
        if pubkeys.is_empty() {
            return Ok(Vec::new());
        }

        // Get the current block number once for all queries
        let block_number = self.get_last_block_number().await?;

        // Process all pubkeys in a single batch operation
        let mut account_infos = Vec::with_capacity(pubkeys.len());

        // Get all account indices in a single batch operation if possible
        // For now, we'll process them individually but in parallel
        let mut futures = Vec::with_capacity(pubkeys.len());
        for pubkey in pubkeys {
            let account_tree = self.account_tree.clone();
            let pubkey = *pubkey;
            let block_number_u64 = block_number as u64;

            // Create a future for each pubkey lookup
            let future = async move {
                let account_id = account_tree.index(block_number_u64, pubkey).await?;
                let last_block_number = if let Some(index) = account_id {
                    let account_leaf = account_tree.get_leaf(block_number_u64, index).await?;
                    account_leaf.value as u32
                } else {
                    0
                };

                Ok::<(Option<u64>, u32), ValidityProverError>((account_id, last_block_number))
            };

            futures.push(future);
        }

        // Execute all futures concurrently
        let results = futures::future::join_all(futures).await;

        // Process results
        for result in results {
            match result {
                Ok((account_id, last_block_number)) => {
                    account_infos.push(AccountInfo {
                        block_number,
                        account_id,
                        last_block_number,
                    });
                }
                Err(e) => return Err(e),
            }
        }

        Ok(account_infos)
    }

    pub async fn get_block_number_by_tx_tree_root(
        &self,
        tx_tree_root: Bytes32,
    ) -> Result<Option<u32>, ValidityProverError> {
        let record = sqlx::query!(
            "SELECT block_number FROM tx_tree_roots WHERE tx_tree_root = $1",
            tx_tree_root.to_bytes_be()
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(record.map(|r| r.block_number as u32))
    }

    pub async fn get_block_number_by_tx_tree_root_batch(
        &self,
        tx_tree_roots: &[Bytes32],
    ) -> Result<Vec<Option<u32>>, ValidityProverError> {
        // early return
        if tx_tree_roots.is_empty() {
            return Ok(Vec::new());
        }

        // Create a mapping to preserve the original order
        let mut result_map: HashMap<Vec<u8>, Option<u32>> = tx_tree_roots
            .iter()
            .map(|root| (root.to_bytes_be(), None))
            .collect();

        // Prepare the values for the SQL query
        let values_params: Vec<String> = tx_tree_roots
            .iter()
            .enumerate()
            .map(|(i, _)| format!("(${})", i + 1))
            .collect();

        // Build the query with a VALUES clause
        let query = format!(
            r#"
            WITH input_roots(tx_tree_root) AS (
                VALUES {}
            )
            SELECT i.tx_tree_root, t.block_number
            FROM input_roots i
            LEFT JOIN tx_tree_roots t ON i.tx_tree_root = t.tx_tree_root
            "#,
            values_params.join(",")
        );

        // Prepare the query arguments
        let mut query_builder = sqlx::query_as::<_, (Vec<u8>, Option<i32>)>(&query);
        for root in tx_tree_roots {
            query_builder = query_builder.bind(root.to_bytes_be());
        }

        // Execute the query
        let records = query_builder.fetch_all(&self.pool).await?;

        // Update the result map with the query results
        for (root, block_number) in records {
            if let Some(bn) = block_number {
                result_map.insert(root, Some(bn as u32));
            }
        }

        // Return results in the same order as the input
        Ok(tx_tree_roots
            .iter()
            .map(|root| result_map[&root.to_bytes_be()])
            .collect())
    }

    pub async fn get_validity_witness(
        &self,
        block_number: u32,
    ) -> Result<ValidityWitness, ValidityProverError> {
        if block_number == 0 {
            return Ok(ValidityWitness::genesis());
        }
        let record = sqlx::query!(
            r#"
            SELECT validity_witness
            FROM validity_state
            WHERE block_number = $1
            "#,
            block_number as i32,
        )
        .fetch_optional(&self.pool)
        .await?
        .ok_or(ValidityProverError::ValidityWitnessNotFound(block_number))?;
        let validity_witness: ValidityWitness = bincode::deserialize(&record.validity_witness)?;
        Ok(validity_witness)
    }

    pub async fn get_block_merkle_proof(
        &self,
        root_block_number: u32,
        leaf_block_number: u32,
    ) -> Result<BlockHashMerkleProof, ValidityProverError> {
        if leaf_block_number > root_block_number {
            return Err(ValidityProverError::InputError(
                "leaf_block_number should be smaller than root_block_number".to_string(),
            ));
        }
        let proof = self
            .block_tree
            .prove(root_block_number as u64, leaf_block_number as u64)
            .await?;
        Ok(proof)
    }

    async fn get_account_membership_proof(
        &self,
        block_number: u32,
        pubkey: U256,
    ) -> Result<AccountMembershipProof, ValidityProverError> {
        let proof = self
            .account_tree
            .prove_membership(block_number as u64, pubkey)
            .await?;
        Ok(proof)
    }

    pub async fn get_latest_validity_proof_block_number(&self) -> Result<u32, ValidityProverError> {
        let record = sqlx::query!(
            r#"
            SELECT block_number
            FROM validity_proofs
            ORDER BY block_number DESC
            LIMIT 1
            "#,
        )
        .fetch_optional(&self.pool)
        .await?;
        let block_number = record.map(|r| r.block_number as u32).unwrap_or(0);
        Ok(block_number)
    }

    pub async fn get_last_block_number(&self) -> Result<u32, ValidityProverError> {
        let record =
            sqlx::query!("SELECT MAX(block_number) as last_block_number FROM validity_state")
                .fetch_optional(&self.pool)
                .await?;
        let last_block_number = record.and_then(|r| r.last_block_number).unwrap_or(0); // i32

        Ok(last_block_number as u32)
    }

    pub async fn get_deposit_merkle_proof(
        &self,
        block_number: u32,
        deposit_index: u32,
    ) -> Result<DepositMerkleProof, ValidityProverError> {
        let proof = self
            .deposit_hash_tree
            .prove(block_number as u64, deposit_index as u64)
            .await?;
        Ok(IncrementalMerkleProof(MerkleProof {
            siblings: proof.0.siblings,
        }))
    }
}
