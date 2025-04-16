use std::{
    collections::HashMap,
    sync::{Arc, OnceLock},
    time::Duration,
};

use intmax2_client_sdk::external_api::contract::{
    liquidity_contract::LiquidityContract, rollup_contract::RollupContract,
};
use intmax2_interfaces::{
    api::validity_prover::interface::{
        AccountInfo, DepositInfo, TransitionProofTask, TransitionProofTaskResult,
    },
    utils::circuit_verifiers::CircuitVerifiers,
};
use intmax2_zkp::{
    circuits::validity::validity_circuit::ValidityCircuit,
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

use server_common::{
    db::{DbPool, DbPoolConfig},
    redis::task_manager::TaskManager,
};

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
const MAX_TASKS: u32 = 30;

const ADD_TASKS_INTERVAL: u64 = 10;
const GENERATE_VALIDITY_PROOF_INTERVAL: u64 = 2;

#[derive(Clone)]
pub struct Config {
    pub sync_interval: Option<u64>,
}

#[derive(Clone)]
pub struct ValidityProver {
    config: Config,
    manager: Arc<TaskManager<TransitionProofTask, TransitionProofTaskResult>>,
    validity_circuit: Arc<OnceLock<ValidityCircuit<F, C, D>>>,
    observer: Observer,
    account_tree: SqlIndexedMerkleTree,
    block_tree: SqlIncrementalMerkleTree<Bytes32>,
    deposit_hash_tree: SqlIncrementalMerkleTree<DepositHash>,
    pool: DbPool,
}

impl ValidityProver {
    pub async fn new(env: &Env) -> Result<Self, ValidityProverError> {
        let config = Config {
            sync_interval: env.sync_interval,
        };

        let manager = Arc::new(TaskManager::new(
            &env.redis_url,
            "validity_prover",
            env.task_ttl as usize,
            env.heartbeat_interval as usize,
        )?);

        let rollup_contract = RollupContract::new(
            &env.l2_rpc_url,
            env.l2_chain_id,
            env.rollup_contract_address,
            env.rollup_contract_deployed_block_number,
        );
        let liquidity_contract = LiquidityContract::new(
            &env.l1_rpc_url,
            env.l1_chain_id,
            env.liquidity_contract_address,
        );

        let observer = Observer::new(
            rollup_contract,
            liquidity_contract,
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
            manager,
            validity_circuit: Arc::new(OnceLock::new()),
            observer,
            pool,
            account_tree,
            block_tree,
            deposit_hash_tree,
        })
    }

    fn validity_circuit(&self) -> &ValidityCircuit<F, C, D> {
        self.validity_circuit.get_or_init(|| {
            let transition_vd = CircuitVerifiers::load().get_transition_vd();
            ValidityCircuit::new(&transition_vd)
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

        let mut prev_validity_pis = if last_block_number == 0 {
            ValidityWitness::genesis().to_validity_pis().unwrap()
        } else {
            self.get_validity_witness(last_block_number)
                .await?
                .to_validity_pis()
                .unwrap()
        };
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

            let tx_tree_root = full_block.signature.block_sign_payload.tx_tree_root;
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

            // Add a new task to the task manager
            self.manager
                .add_task(
                    block_number,
                    &TransitionProofTask {
                        block_number,
                        prev_validity_pis: prev_validity_pis.clone(),
                        validity_witness: validity_witness.clone(),
                    },
                )
                .await?;
            prev_validity_pis = validity_witness.to_validity_pis().unwrap();
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

    pub async fn get_deposit_info(
        &self,
        deposit_hash: Bytes32,
    ) -> Result<Option<DepositInfo>, ValidityProverError> {
        let deposit_info = self
            .observer
            .get_deposit_info(deposit_hash)
            .await
            .map_err(ValidityProverError::ObserverError)?;
        Ok(deposit_info)
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

    pub async fn get_next_deposit_index(&self) -> Result<u32, ValidityProverError> {
        let deposit_index = self.observer.get_next_deposit_index().await?;
        Ok(deposit_index)
    }

    pub async fn get_latest_included_deposit_index(
        &self,
    ) -> Result<Option<u32>, ValidityProverError> {
        let deposit_index = self.observer.get_latest_included_deposit_index().await?;
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

    async fn generate_validity_proof(&self) -> Result<(), ValidityProverError> {
        // Get the largest block_number and its proof from the validity_proofs table that already exists
        let record = sqlx::query!(
            r#"
            SELECT block_number, proof
            FROM validity_proofs
            ORDER BY block_number DESC
            LIMIT 1
            "#,
        )
        .fetch_optional(&self.pool)
        .await?;
        let (mut last_validity_proof_block_number, mut prev_proof) = match record {
            Some(record) => (record.block_number as u32, {
                let proof: ProofWithPublicInputs<F, C, D> = bincode::deserialize(&record.proof)?;
                Some(proof)
            }),
            None => (0, None),
        };
        let last_block_number = self.get_last_block_number().await?;
        if last_validity_proof_block_number == last_block_number {
            return Ok(());
        }

        loop {
            last_validity_proof_block_number += 1;

            // get result from the task manager
            let result = self
                .manager
                .get_result(last_validity_proof_block_number)
                .await?;
            if result.is_none() {
                log::info!("result not found for {}", last_validity_proof_block_number);
                break;
            }
            log::info!("result found for {}", last_validity_proof_block_number);

            let result = result.unwrap();
            if let Some(error) = result.error {
                return Err(ValidityProverError::TaskError(format!(
                    "Error in block number {}: {}",
                    last_validity_proof_block_number, error
                )));
            }
            if result.proof.is_none() {
                return Err(ValidityProverError::TaskError(format!(
                    "Proof is missing for block number {}",
                    last_validity_proof_block_number
                )));
            }
            let transition_proof = result.proof.unwrap();
            let validity_proof = self
                .validity_circuit()
                .prove(&transition_proof, &prev_proof)
                .map_err(|e| ValidityProverError::FailedToGenerateValidityProof(e.to_string()))?;
            log::info!(
                "validity proof generated: {}",
                last_validity_proof_block_number
            );
            // Add a new validity proof to the validity_proofs table
            sqlx::query!(
                r#"
                INSERT INTO validity_proofs (block_number, proof)
                VALUES ($1, $2)
                ON CONFLICT (block_number)
                DO UPDATE SET proof = $2
                "#,
                last_validity_proof_block_number as i32,
                bincode::serialize(&validity_proof)?,
            )
            .execute(&self.pool)
            .await?;
            prev_proof = Some(validity_proof);

            // Remove the result from the task manager
            self.manager
                .remove_old_tasks(last_validity_proof_block_number)
                .await?;
        }

        Ok(())
    }

    // This function is used to setup all tasks in the task manager
    async fn add_tasks(&self) -> Result<(), ValidityProverError> {
        let last_validity_prover_block_number =
            self.get_latest_validity_proof_block_number().await?;
        let last_block_number = self.get_last_block_number().await?;
        if last_validity_prover_block_number == last_block_number {
            // early return
            return Ok(());
        }

        let to_block_number = last_block_number.min(last_validity_prover_block_number + MAX_TASKS);
        let mut prev_validity_pis = self
            .get_validity_witness(last_validity_prover_block_number)
            .await?
            .to_validity_pis()
            .unwrap();
        for block_number in (last_validity_prover_block_number + 1)..=to_block_number {
            if self.manager.check_task_exists(block_number).await? {
                break;
            }
            let validity_witness = self.get_validity_witness(block_number).await?;
            let task = TransitionProofTask {
                block_number,
                prev_validity_pis: prev_validity_pis.clone(),
                validity_witness: validity_witness.clone(),
            };

            // check again if the validity proof is already generated
            let current_last_validity_prover_block_number =
                self.get_latest_validity_proof_block_number().await?;
            if block_number <= current_last_validity_prover_block_number {
                break;
            }
            log::info!(
                "adding task for block number {} > validity block number {}",
                block_number,
                current_last_validity_prover_block_number
            );
            self.manager.add_task(block_number, &task).await?;

            prev_validity_pis = validity_witness.to_validity_pis().unwrap();
        }

        Ok(())
    }

    pub(crate) async fn job(&self) -> Result<(), ValidityProverError> {
        if self.config.sync_interval.is_none() {
            // If sync_interval is not set, we don't run the sync task
            return Ok(());
        }
        let sync_interval = self.config.sync_interval.unwrap();

        // clear all tasks
        self.manager.clear_all().await?;

        // generate validity proof job
        let self_clone = self.clone();
        actix_web::rt::spawn(async move {
            loop {
                let self_clone = self_clone.clone();
                let generate_validity_proof_result = actix_web::rt::spawn(async move {
                    if let Err(e) = self_clone.generate_validity_proof().await {
                        log::error!("Error in generate validity proof: {:?}", e);
                    }
                })
                .await;
                if let Err(e) = generate_validity_proof_result {
                    log::error!("Panic error in generate validity proof: {:?}", e);
                }
                tokio::time::sleep(Duration::from_secs(GENERATE_VALIDITY_PROOF_INTERVAL)).await;
            }
        });

        // add tasks job
        let self_clone = self.clone();
        actix_web::rt::spawn(async move {
            loop {
                let self_clone = self_clone.clone();
                let add_task_result = actix_web::rt::spawn(async move {
                    if let Err(e) = self_clone.add_tasks().await {
                        log::error!("Error in add tasks: {:?}", e);
                    }
                })
                .await;
                if let Err(e) = add_task_result {
                    log::error!("Panic error in add tasks: {:?}", e);
                }
                tokio::time::sleep(Duration::from_secs(ADD_TASKS_INTERVAL)).await;
            }
        });

        let self_clone = self.clone();
        actix_web::rt::spawn(async move {
            loop {
                let self_clone = self_clone.clone();
                let sync_result = actix_web::rt::spawn(async move {
                    if let Err(e) = self_clone.sync().await {
                        log::error!("Error in sync: {:?}", e);
                    }
                })
                .await;
                if let Err(e) = sync_result {
                    log::error!("Panic error in sync: {:?}", e);
                }
                tokio::time::sleep(Duration::from_secs(sync_interval)).await;
            }
        });

        let manager = self.manager.clone();
        tokio::spawn(async move {
            let manager = manager.clone();
            if let Err(e) = manager.cleanup_inactive_tasks().await {
                log::error!("Error in task manager: {:?}", e);
            }
        });

        Ok(())
    }
}
