use std::{sync::Arc, time::Duration};

use intmax2_interfaces::{
    api::validity_prover::interface::{TransitionProofTask, TransitionProofTaskResult},
    utils::circuit_verifiers::CircuitVerifiers,
};
use intmax2_zkp::circuits::validity::validity_circuit::ValidityCircuit;
use plonky2::{
    field::goldilocks_field::GoldilocksField,
    plonk::{config::PoseidonGoldilocksConfig, proof::ProofWithPublicInputs},
};

use server_common::{
    db::{DbPool, DbPoolConfig},
    redis::task_manager::TaskManager,
};

use crate::Env;

use super::error::ProverCoordinatorError;

type F = GoldilocksField;
type C = PoseidonGoldilocksConfig;
const D: usize = 2;

const CLEANUP_INTERVAL: u64 = 10;

type Result<T> = std::result::Result<T, ProverCoordinatorError>;

#[derive(Clone, Debug)]
pub struct ProverCoordinator {
    pub validity_circuit: Arc<ValidityCircuit<F, C, D>>,
    pub pool: DbPool,
    pub manager: Arc<TaskManager<TransitionProofTask, TransitionProofTaskResult>>,
}

impl ProverCoordinator {
    pub async fn new(env: &Env) -> Result<Self> {
        let pool = DbPool::from_config(&DbPoolConfig {
            max_connections: env.database_max_connections,
            idle_timeout: env.database_timeout,
            url: env.database_url.clone(),
        })
        .await?;
        let transition_vd = CircuitVerifiers::load().get_transition_vd();
        let validity_circuit = ValidityCircuit::new(&transition_vd);
        let manager = Arc::new(TaskManager::new(
            &env.redis_url,
            "validity_prover",
            env.ttl as usize,
            0, // dummy value
        )?);
        Ok(Self {
            validity_circuit: Arc::new(validity_circuit),
            pool,
            manager,
        })
    }

    pub async fn generate_validity_proof(&self) -> Result<()> {
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

        loop {
            last_validity_proof_block_number += 1;
            let result = self
                .manager
                .get_result(last_validity_proof_block_number)
                .await?;
            if result.is_none() {
                break;
            }
            let result = result.unwrap();
            if let Some(error) = result.error {
                return Err(ProverCoordinatorError::TaskError(format!(
                    "Error in block number {}: {}",
                    last_validity_proof_block_number, error
                )));
            }
            if result.proof.is_none() {
                return Err(ProverCoordinatorError::TaskError(format!(
                    "Proof is missing for block number {}",
                    last_validity_proof_block_number
                )));
            }
            let transition_proof = result.proof.unwrap();
            let validity_proof = self
                .validity_circuit
                .prove(&transition_proof, &prev_proof)
                .map_err(|e| {
                    ProverCoordinatorError::FailedToGenerateValidityProof(e.to_string())
                })?;
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
        }

        Ok(())
    }

    pub fn job(&self) {
        let manager = self.manager.clone();
        let _cleanup_handler = tokio::spawn(async move {
            loop {
                manager.cleanup_inactive_workers().await.unwrap();
                tokio::time::sleep(Duration::from_secs(CLEANUP_INTERVAL)).await;
            }
        });
        let coordinator = self.clone();
        let _validity_prove_handler = tokio::spawn(async move {
            loop {
                coordinator.generate_validity_proof().await.unwrap();
                tokio::time::sleep(Duration::from_secs(10)).await;
            }
        });
    }
}
