use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use intmax2_client_sdk::external_api::contract::rollup_contract::RollupContract;
use intmax2_interfaces::api::validity_prover::interface::{AccountInfo, DepositInfo};
use intmax2_zkp::{
    common::{
        block::Block,
        trees::{
            account_tree::AccountMembershipProof, block_hash_tree::BlockHashMerkleProof,
            deposit_tree::DepositMerkleProof,
        },
        witness::{update_witness::UpdateWitness, validity_witness::ValidityWitness},
    },
    constants::{ACCOUNT_TREE_HEIGHT, BLOCK_HASH_TREE_HEIGHT, DEPOSIT_TREE_HEIGHT},
    ethereum_types::{bytes32::Bytes32, u256::U256, u32limb_trait::U32LimbTrait as _},
    utils::trees::{incremental_merkle_tree::IncrementalMerkleProof, merkle_tree::MerkleProof},
};

use crate::trees::merkle_tree::IncrementalMerkleTreeClient;

use plonky2::{
    field::goldilocks_field::GoldilocksField,
    plonk::{config::PoseidonGoldilocksConfig, proof::ProofWithPublicInputs},
};

use server_common::db::{DbPool, DbPoolConfig};
use tokio::time::interval;

use super::{error::ValidityProverError, observer::Observer};
use crate::{
    trees::{
        deposit_hash::DepositHash,
        merkle_tree::{
            sql_incremental_merkle_tree::SqlIncrementalMerkleTree,
            sql_indexed_merkle_tree::SqlIndexedMerkleTree, IndexedMerkleTreeClient,
        },
        update::{to_block_witness, update_trees},
    },
    Env,
};

type F = GoldilocksField;
type C = PoseidonGoldilocksConfig;
const D: usize = 2;

const ACCOUNT_DB_TAG: u32 = 1;
const BLOCK_DB_TAG: u32 = 2;
const DEPOSIT_DB_TAG: u32 = 3;

#[derive(Clone)]
pub struct Config {
    pub sync_interval: u64,
}

#[derive(Clone)]
pub struct WitnessGenerator {
    config: Config,
    observer: Observer,
    account_tree: SqlIndexedMerkleTree,
    block_tree: SqlIncrementalMerkleTree<Bytes32>,
    deposit_hash_tree: SqlIncrementalMerkleTree<DepositHash>,
    pool: DbPool,
}

impl WitnessGenerator {
    pub async fn new(env: &Env) -> Result<Self, ValidityProverError> {
        let config = Config {
            sync_interval: env.sync_interval,
        };

        let rollup_contract = RollupContract::new(
            &env.l2_rpc_url,
            env.l2_chain_id,
            env.rollup_contract_address,
            env.rollup_contract_deployed_block_number,
        );
        let observer = Observer::new(
            rollup_contract,
            &env.database_url,
            env.database_max_connections,
            env.database_timeout,
        )
        .await?;

        let pool = sqlx::Pool::connect(&env.database_url).await?;

        let account_tree =
            SqlIndexedMerkleTree::new(pool.clone(), ACCOUNT_DB_TAG, ACCOUNT_TREE_HEIGHT);
        account_tree.initialize().await?;
        let block_tree = SqlIncrementalMerkleTree::<Bytes32>::new(
            pool.clone(),
            BLOCK_DB_TAG,
            BLOCK_HASH_TREE_HEIGHT,
        );
        let last_timestamp = block_tree.get_last_timestamp().await?;
        if last_timestamp == 0 {
            let len = block_tree.len(last_timestamp).await?;
            if len == 0 {
                block_tree
                    .push(last_timestamp, Block::genesis().hash())
                    .await?;
            }
        }
        let deposit_hash_tree = SqlIncrementalMerkleTree::<DepositHash>::new(
            pool.clone(),
            DEPOSIT_DB_TAG,
            DEPOSIT_TREE_HEIGHT,
        );
        log::info!("block tree len: {}", block_tree.len(last_timestamp).await?);
        log::info!(
            "deposit tree len: {}",
            deposit_hash_tree.len(last_timestamp).await?
        );
        log::info!(
            "account tree len: {}",
            account_tree.len(last_timestamp).await?
        );

        let pool = DbPool::from_config(&DbPoolConfig {
            max_connections: env.database_max_connections,
            idle_timeout: env.database_timeout,
            url: env.database_url.clone(),
        })
        .await?;

        Ok(Self {
            config,
            observer,
            pool,
            account_tree,
            block_tree,
            deposit_hash_tree,
        })
    }

    pub async fn sync_observer(&self) -> Result<(), ValidityProverError> {
        self.observer.sync().await?;
        Ok(())
    }

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

    pub async fn sync(&self) -> Result<(), ValidityProverError> {
        log::info!(
            "Start sync validity prover: current block number {}, observer block number {}, validity proof block number: {}",
            self.get_last_block_number().await?,
            self.observer.get_next_block_number().await? - 1,
            self.get_latest_validity_proof_block_number().await?,
        );
        self.sync_observer().await?;

        let last_block_number = self.get_last_block_number().await?;
        let next_block_number = self.observer.get_next_block_number().await?;

        for block_number in (last_block_number + 1)..next_block_number {
            log::info!(
                "Sync validity prover: syncing block number {}",
                block_number
            );

            let full_block = self.observer.get_full_block(block_number).await?;

            let deposit_events = self
                .observer
                .get_deposits_between_blocks(block_number)
                .await?;
            // Caution! This change the state of the deposit hash tree
            for event in deposit_events {
                self.deposit_hash_tree
                    .push(block_number as u64, DepositHash(event.deposit_hash))
                    .await?;
            }
            let deposit_tree_root = self.deposit_hash_tree.get_root(block_number as u64).await?;
            if full_block.block.deposit_tree_root != deposit_tree_root {
                // Reset merkle tree
                self.reset_merkle_tree(block_number).await?;
                return Err(ValidityProverError::DepositTreeRootMismatch(
                    full_block.block.deposit_tree_root,
                    deposit_tree_root,
                ));
            }

            let block_witness = to_block_witness(
                &full_block,
                block_number as u64,
                &self.account_tree,
                &self.block_tree,
            )
            .await
            .map_err(|e| ValidityProverError::BlockWitnessGenerationError(e.to_string()))?;

            // Caution! This change the state of the account tree and block tree
            // TODO: atomic update
            let validity_witness = match update_trees(
                &block_witness,
                block_number as u64,
                &self.account_tree,
                &self.block_tree,
            )
            .await
            {
                Ok(w) => w,
                Err(e) => {
                    self.reset_merkle_tree(block_number).await?;
                    return Err(ValidityProverError::FailedToUpdateTrees(e.to_string()));
                }
            };
            // Update database state
            let mut tx = self.pool.begin().await?;
            sqlx::query!(
                "INSERT INTO validity_state (block_number, validity_witness) VALUES ($1, $2)",
                block_number as i32,
                bincode::serialize(&validity_witness)?,
            )
            .execute(tx.as_mut())
            .await?;

            sqlx::query!(
                "INSERT INTO prover_tasks (block_number, assigned, completed) VALUES ($1, FALSE, FALSE)
                 ON CONFLICT (block_number) DO NOTHING",
                block_number as i32
            )
            .execute(tx.as_mut()).await?;

            let tx_tree_root = full_block.signature.tx_tree_root;
            if tx_tree_root != Bytes32::default()
                && validity_witness.to_validity_pis().unwrap().is_valid_block
            {
                sqlx::query!(
                    "INSERT INTO tx_tree_roots (tx_tree_root, block_number) VALUES ($1, $2)
                     ON CONFLICT (tx_tree_root) DO UPDATE SET block_number = $2",
                    tx_tree_root.to_bytes_be(),
                    block_number as i32
                )
                .execute(tx.as_mut())
                .await?;
            }

            tx.commit().await?;
        }
        log::info!("End of sync validity prover");
        Ok(())
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

    pub async fn get_account_id(&self, pubkey: U256) -> Result<Option<u64>, ValidityProverError> {
        let last_block_number = self.get_last_block_number().await?;
        let index = self
            .account_tree
            .index(last_block_number as u64, pubkey)
            .await?;
        Ok(index)
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
        let mut account_infos = Vec::new();
        for pubkey in pubkeys {
            let account_info = self.get_account_info(*pubkey).await?;
            account_infos.push(account_info);
        }
        Ok(account_infos)
    }

    pub async fn get_deposit_info(
        &self,
        deposit_hash: Bytes32,
    ) -> Result<Option<DepositInfo>, ValidityProverError> {
        let deposit_info = self.observer.get_deposit_info(deposit_hash).await?;
        Ok(deposit_info)
    }

    pub async fn get_deposit_info_batch(
        &self,
        deposit_hashes: &[Bytes32],
    ) -> Result<Vec<Option<DepositInfo>>, ValidityProverError> {
        let mut deposit_infos = Vec::new();
        for deposit_hash in deposit_hashes {
            let deposit_info = self.observer.get_deposit_info(*deposit_hash).await?;
            deposit_infos.push(deposit_info);
        }
        Ok(deposit_infos)
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

        let root_bytes: Vec<Vec<u8>> = tx_tree_roots.iter().map(|r| r.to_bytes_be()).collect();

        let records = sqlx::query!(
            r#"
            SELECT tx_tree_root, block_number 
            FROM tx_tree_roots 
            WHERE tx_tree_root = ANY($1)
            "#,
            &root_bytes as &[Vec<u8>]
        )
        .fetch_all(&self.pool)
        .await?;

        let block_map: HashMap<Vec<u8>, i32> = records
            .into_iter()
            .map(|r| (r.tx_tree_root, r.block_number))
            .collect();

        Ok(tx_tree_roots
            .iter()
            .map(|root| {
                block_map
                    .get(&root.to_bytes_be())
                    .map(|&block_number| block_number as u32)
            })
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

    pub async fn get_next_deposit_index(&self) -> Result<u32, ValidityProverError> {
        let deposit_index = self.observer.get_next_deposit_index().await?;
        Ok(deposit_index)
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

    async fn reset_merkle_tree(&self, block_number: u32) -> Result<(), ValidityProverError> {
        log::warn!("Reset merkle tree from block number {}", block_number);
        self.account_tree.reset(block_number as u64).await?;
        self.block_tree.reset(block_number as u64).await?;
        self.deposit_hash_tree.reset(block_number as u64).await?;
        Ok(())
    }

    pub fn job(self) {
        let is_syncing = Arc::new(AtomicBool::new(false));
        let is_syncing_clone = is_syncing.clone();
        actix_web::rt::spawn(async move {
            let mut interval = interval(Duration::from_secs(self.config.sync_interval));
            loop {
                interval.tick().await;

                // Skip if previous task is still running
                if is_syncing_clone.load(Ordering::SeqCst) {
                    log::warn!("Previous sync task is still running, skipping this interval");
                    continue;
                }

                is_syncing_clone.store(true, Ordering::SeqCst);

                match self.sync().await {
                    Ok(_) => {
                        log::debug!("Sync task completed successfully");
                    }
                    Err(e) => {
                        log::error!("Error in sync task: {:?}", e);
                    }
                }
                // Reset the flag after task completion
                is_syncing_clone.store(false, Ordering::SeqCst);
            }
        });
    }
}
