use super::{
    error::ValidityProverError, leader_election::LeaderElection, observer_api::ObserverApi,
    rate_manager::RateManager,
};
use crate::{
    app::setting_consistency::SettingConsistency,
    trees::{
        deposit_hash::DepositHash,
        merkle_tree::{
            sql_incremental_merkle_tree::SqlIncrementalMerkleTree,
            sql_indexed_merkle_tree::SqlIndexedMerkleTree, IncrementalMerkleTreeClient,
            IndexedMerkleTreeClient,
        },
        update::{to_block_witness, update_trees},
    },
    EnvVar,
};
use intmax2_interfaces::{
    api::validity_prover::interface::{TransitionProofTask, TransitionProofTaskResult},
    utils::circuit_verifiers::CircuitVerifiers,
};
use intmax2_zkp::{
    circuits::validity::validity_circuit::ValidityCircuit,
    common::{block::Block, witness::validity_witness::ValidityWitness},
    constants::{ACCOUNT_TREE_HEIGHT, BLOCK_HASH_TREE_HEIGHT, DEPOSIT_TREE_HEIGHT},
    ethereum_types::{bytes32::Bytes32, u32limb_trait::U32LimbTrait as _},
};
use plonky2::{
    field::goldilocks_field::GoldilocksField,
    plonk::{config::PoseidonGoldilocksConfig, proof::ProofWithPublicInputs},
};
use server_common::{
    db::{DbPool, DbPoolConfig},
    redis::task_manager::TaskManager,
};
use sqlx::Pool;
use std::{
    sync::{Arc, OnceLock},
    time::Duration,
};
use tracing::instrument;

type F = GoldilocksField;
type C = PoseidonGoldilocksConfig;
const D: usize = 2;

const ACCOUNT_DB_TAG: u32 = 1;
const BLOCK_DB_TAG: u32 = 2;
const DEPOSIT_DB_TAG: u32 = 3;
const ACCOUNT_BACKUP_DB_TAG: u32 = 11;
const BLOCK_BACKUP_DB_TAG: u32 = 12;
const DEPOSIT_BACKUP_DB_TAG: u32 = 13;

const MAX_TASKS: u32 = 30;

pub const SYNC_VALIDITY_WITNESS_KEY: &str = "sync_validity_witness";
pub const GENERATE_VALIDITY_PROOF_KEY: &str = "generate_validity_proof";
pub const ADD_TASKS_KEY: &str = "add_tasks";
pub const CLEANUP_INACTIVE_TASKS_KEY: &str = "cleanup_inactive_tasks";

#[derive(Clone, Debug)]
pub struct ValidityProverConfig {
    pub is_sync_mode: bool,
    pub witness_sync_interval: u64,
    pub validity_proof_interval: u64,
    pub add_tasks_interval: u64,
    pub cleanup_inactive_tasks_interval: u64,
    pub validity_prover_restart_interval: u64,
}

#[derive(Clone)]
pub struct ValidityProver {
    pub config: ValidityProverConfig,
    pub manager: Arc<TaskManager<TransitionProofTask, TransitionProofTaskResult>>,
    pub validity_circuit: Arc<OnceLock<ValidityCircuit<F, C, D>>>,
    pub observer_api: ObserverApi,
    pub leader_election: LeaderElection,
    pub rate_manager: RateManager,
    pub account_tree: SqlIndexedMerkleTree,
    pub block_tree: SqlIncrementalMerkleTree<Bytes32>,
    pub deposit_hash_tree: SqlIncrementalMerkleTree<DepositHash>,
    pub account_tree_backup: SqlIndexedMerkleTree,
    pub block_tree_backup: SqlIncrementalMerkleTree<Bytes32>,
    pub deposit_hash_tree_backup: SqlIncrementalMerkleTree<DepositHash>,
    pub pool: DbPool,
}

impl ValidityProver {
    pub async fn new(
        env: &EnvVar,
        observer_api: ObserverApi,
        leader_election: LeaderElection,
        rate_manager: RateManager,
    ) -> Result<Self, ValidityProverError> {
        let config = ValidityProverConfig {
            is_sync_mode: env.is_sync_mode,
            witness_sync_interval: env.witness_sync_interval,
            validity_proof_interval: env.validity_proof_interval,
            add_tasks_interval: env.add_tasks_interval,
            cleanup_inactive_tasks_interval: env.cleanup_inactive_tasks_interval,
            validity_prover_restart_interval: env.validity_prover_restart_interval,
        };
        tracing::info!("ValidityProverConfig: {:?}", config);
        let manager = Arc::new(TaskManager::new(
            &env.redis_url,
            "validity_prover",
            env.task_ttl as usize,
            env.heartbeat_interval as usize,
        )?);
        let pool = Pool::connect(&env.database_url).await?;
        // check consistency
        {
            let setting_consistency = SettingConsistency::new(pool.clone());
            setting_consistency
                .check_consistency(env.rollup_contract_address, env.liquidity_contract_address)
                .await?;
        }
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
        tracing::info!("block tree len: {}", block_tree.len(last_timestamp).await?);
        tracing::info!(
            "deposit tree len: {}",
            deposit_hash_tree.len(last_timestamp).await?
        );
        tracing::info!(
            "account tree len: {}",
            account_tree.len(last_timestamp).await?
        );
        let account_tree_backup =
            SqlIndexedMerkleTree::new(pool.clone(), ACCOUNT_BACKUP_DB_TAG, ACCOUNT_TREE_HEIGHT);
        let block_tree_backup = SqlIncrementalMerkleTree::<Bytes32>::new(
            pool.clone(),
            BLOCK_BACKUP_DB_TAG,
            BLOCK_HASH_TREE_HEIGHT,
        );
        let deposit_hash_tree_backup = SqlIncrementalMerkleTree::<DepositHash>::new(
            pool.clone(),
            DEPOSIT_BACKUP_DB_TAG,
            DEPOSIT_TREE_HEIGHT,
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
            observer_api,
            leader_election,
            rate_manager,
            pool,
            account_tree,
            block_tree,
            deposit_hash_tree,
            account_tree_backup,
            block_tree_backup,
            deposit_hash_tree_backup,
        })
    }

    fn validity_circuit(&self) -> &ValidityCircuit<F, C, D> {
        self.validity_circuit.get_or_init(|| {
            let transition_vd = CircuitVerifiers::load().get_transition_vd();
            ValidityCircuit::new(&transition_vd)
        })
    }

    #[instrument(skip(self))]
    async fn sync_validity_witness(&self) -> Result<(), ValidityProverError> {
        self.leader_election.wait_for_leadership().await?;

        let observer_block_number = self.observer_api.get_local_last_block_number().await?;
        let last_block_number = self.get_last_block_number().await?;
        let next_block_number = observer_block_number + 1;
        let mut prev_validity_pis = if last_block_number == 0 {
            ValidityWitness::genesis().to_validity_pis().unwrap()
        } else {
            self.get_validity_witness(last_block_number)
                .await?
                .to_validity_pis()
                .unwrap()
        };
        for block_number in (last_block_number + 1)..next_block_number {
            tracing::info!(
                "sync_validity_witness: syncing block number {block_number}, observer block number {observer_block_number}",
            );
            self.rate_manager
                .emit_heartbeat(SYNC_VALIDITY_WITNESS_KEY)
                .await?;
            let full_block_with_meta = self
                .observer_api
                .get_full_block_with_meta(block_number)
                .await?
                .unwrap();
            let full_block = full_block_with_meta.full_block;
            let deposit_events = self
                .observer_api
                .get_deposits_between_blocks(block_number)
                .await?;
            if deposit_events.is_none() {
                // not ready yet
                return Ok(());
            }
            let deposit_events = deposit_events.unwrap();
            // Caution! This change the state of the deposit hash tree
            for event in deposit_events {
                self.deposit_hash_tree
                    .push(block_number as u64, DepositHash(event.deposit_hash))
                    .await?;
            }
            let deposit_tree_root = self.deposit_hash_tree.get_root(block_number as u64).await?;
            if full_block.block.deposit_tree_root != deposit_tree_root {
                self.reset_state().await?;
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
                    self.reset_state().await?;
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
        Ok(())
    }

    // Reset the state of the trees which are not synced with the validity witness
    #[instrument(skip(self))]
    async fn reset_state(&self) -> Result<(), ValidityProverError> {
        let reset_block_number = self.get_last_block_number().await? as u64 + 1;
        tracing::warn!("Reset state: reset block number {}", reset_block_number);
        self.account_tree.reset(reset_block_number).await?;
        self.block_tree.reset(reset_block_number).await?;
        self.deposit_hash_tree.reset(reset_block_number).await?;
        Ok(())
    }

    #[instrument(skip(self))]
    async fn generate_validity_proof(&self) -> Result<(), ValidityProverError> {
        self.leader_election.wait_for_leadership().await?;
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
            self.rate_manager
                .emit_heartbeat(GENERATE_VALIDITY_PROOF_KEY)
                .await?;
            last_validity_proof_block_number += 1;

            // get result from the task manager
            let result = self
                .manager
                .get_result(last_validity_proof_block_number)
                .await?;
            if result.is_none() {
                break;
            }
            tracing::info!("result found for {}", last_validity_proof_block_number);

            let result = result.unwrap();
            if let Some(error) = result.error {
                return Err(ValidityProverError::TaskError(format!(
                    "Error in block number {last_validity_proof_block_number}: {error}"
                )));
            }
            if result.proof.is_none() {
                return Err(ValidityProverError::TaskError(format!(
                    "Proof is missing for block number {last_validity_proof_block_number}"
                )));
            }
            let transition_proof = result.proof.unwrap();
            let validity_proof = self
                .validity_circuit()
                .prove(&transition_proof, &prev_proof)
                .map_err(|e| ValidityProverError::FailedToGenerateValidityProof(e.to_string()))?;
            tracing::info!(
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

    #[instrument(skip(self))]
    async fn add_tasks(&self) -> Result<(), ValidityProverError> {
        self.leader_election.wait_for_leadership().await?;
        let last_validity_prover_block_number =
            self.get_latest_validity_proof_block_number().await?;
        let last_block_number = self.get_last_block_number().await?;
        // last_validity_prover_block_number > last_block_number happens when the validity proofs are
        // inserted into the database from backup.
        if last_validity_prover_block_number >= last_block_number {
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
            self.rate_manager.emit_heartbeat(ADD_TASKS_KEY).await?;
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
            tracing::info!(
                "adding task for block number {} > validity block number {}",
                block_number,
                current_last_validity_prover_block_number
            );
            self.manager.add_task(block_number, &task).await?;

            prev_validity_pis = validity_witness.to_validity_pis().unwrap();
        }

        Ok(())
    }

    #[instrument(skip(self))]
    async fn sync_validity_witness_loop(&self) -> Result<(), ValidityProverError> {
        let mut interval =
            tokio::time::interval(Duration::from_secs(self.config.witness_sync_interval));
        loop {
            interval.tick().await;
            self.rate_manager
                .emit_heartbeat(SYNC_VALIDITY_WITNESS_KEY)
                .await?;
            self.sync_validity_witness().await?;
        }
    }

    #[instrument(skip(self))]
    async fn generate_validity_proof_loop(&self) -> Result<(), ValidityProverError> {
        let mut interval =
            tokio::time::interval(Duration::from_secs(self.config.validity_proof_interval));
        loop {
            interval.tick().await;
            self.rate_manager
                .emit_heartbeat(GENERATE_VALIDITY_PROOF_KEY)
                .await?;
            self.generate_validity_proof().await?;
        }
    }

    #[instrument(skip(self))]
    async fn add_tasks_loop(&self) -> Result<(), ValidityProverError> {
        let mut interval =
            tokio::time::interval(Duration::from_secs(self.config.add_tasks_interval));
        loop {
            interval.tick().await;
            self.rate_manager.emit_heartbeat(ADD_TASKS_KEY).await?;
            self.add_tasks().await?;
        }
    }

    #[instrument(skip(self))]
    async fn cleanup_inactive_tasks_loop(&self) -> Result<(), ValidityProverError> {
        let mut interval = tokio::time::interval(Duration::from_secs(
            self.config.cleanup_inactive_tasks_interval,
        ));
        loop {
            interval.tick().await;
            self.rate_manager
                .emit_heartbeat(CLEANUP_INACTIVE_TASKS_KEY)
                .await?;
            self.manager.cleanup_inactive_tasks().await?;
        }
    }

    pub async fn start_all_jobs(&self) -> Result<(), ValidityProverError> {
        // clear all tasks
        self.manager.clear_all().await?;

        let this = Arc::new(self.clone());

        let restart_duration = Duration::from_secs(self.config.validity_prover_restart_interval);

        // generate validity proof job
        let this_clone = this.clone();
        actix_web::rt::spawn(async move {
            // restart loop
            loop {
                let this_clone = this_clone.clone();
                let handler =
                    actix_web::rt::spawn(
                        async move { this_clone.generate_validity_proof_loop().await },
                    );
                match handler.await {
                    Ok(Ok(_)) => {
                        tracing::error!("generate_validity_proof_loop finished");
                    }
                    Ok(Err(e)) => {
                        tracing::error!("generate_validity_proof_loop error: {:?}", e);
                    }
                    Err(e) => {
                        tracing::error!("generate_validity_proof_loop panic: {:?}", e);
                    }
                }
                tokio::time::sleep(restart_duration).await;
            }
        });

        // add tasks job
        let this_clone = this.clone();
        actix_web::rt::spawn(async move {
            // restart loop
            loop {
                let this_clone = this_clone.clone();
                let handler =
                    actix_web::rt::spawn(async move { this_clone.add_tasks_loop().await });
                match handler.await {
                    Ok(Ok(_)) => {
                        tracing::error!("add_tasks_loop finished");
                    }
                    Ok(Err(e)) => {
                        tracing::error!("add_tasks_loop error: {:?}", e);
                    }
                    Err(e) => {
                        tracing::error!("add_tasks_loop panic: {:?}", e);
                    }
                }
                tokio::time::sleep(restart_duration).await;
            }
        });

        // sync validity witness job
        let this_clone = this.clone();
        actix_web::rt::spawn(async move {
            // restart loop
            loop {
                let this_clone = this_clone.clone();
                // using actix_web::rt::spawn because self is not `Send`
                let handler =
                    actix_web::rt::spawn(
                        async move { this_clone.sync_validity_witness_loop().await },
                    );
                match handler.await {
                    Ok(Ok(_)) => {
                        tracing::error!("sync_validity_witness_loop finished");
                    }
                    Ok(Err(e)) => {
                        tracing::error!("sync_validity_witness_loop error: {:?}", e);
                    }
                    Err(e) => {
                        tracing::error!("sync_validity_witness_loop panic: {:?}", e);
                    }
                }
                tokio::time::sleep(restart_duration).await;
            }
        });

        // cleanup inactive tasks job
        let this_clone = this.clone();
        tokio::spawn(async move {
            // restart loop
            loop {
                let this_clone = this_clone.clone();
                let handler =
                    tokio::spawn(async move { this_clone.cleanup_inactive_tasks_loop().await });
                match handler.await {
                    Ok(Ok(_)) => {
                        tracing::error!("cleanup_inactive_tasks_loop finished");
                    }
                    Ok(Err(e)) => {
                        tracing::error!("cleanup_inactive_tasks_loop error: {:?}", e);
                    }
                    Err(e) => {
                        tracing::error!("cleanup_inactive_tasks_loop panic: {:?}", e);
                    }
                }
                tokio::time::sleep(restart_duration).await;
            }
        });

        Ok(())
    }
}
