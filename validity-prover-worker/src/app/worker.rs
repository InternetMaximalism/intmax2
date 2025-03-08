use std::sync::Arc;

use intmax2_interfaces::api::validity_prover::interface::{
    TransitionProofTask, TransitionProofTaskResult,
};
use intmax2_zkp::circuits::validity::transition::processor::TransitionProcessor;
use plonky2::{field::goldilocks_field::GoldilocksField, plonk::config::PoseidonGoldilocksConfig};
use server_common::redis::task_manager::TaskManager;
use tokio::task::JoinHandle;
use uuid::Uuid;

use crate::EnvVar;

use super::error::WorkerError;

type F = GoldilocksField;
type C = PoseidonGoldilocksConfig;
const D: usize = 2;

const TASK_POLLING_INTERVAL: u64 = 1;

type Result<T> = std::result::Result<T, WorkerError>;

#[derive(Clone)]
struct Config {
    heartbeat_interval: u64,
}

#[derive(Clone)]
pub struct Worker {
    config: Config,
    transition_processor: Arc<TransitionProcessor<F, C, D>>,
    manager: Arc<TaskManager<TransitionProofTask, TransitionProofTaskResult>>,
    worker_id: String,
}

impl Worker {
    pub fn new(
        env: &EnvVar,
        transition_processor: Arc<TransitionProcessor<F, C, D>>,
    ) -> Result<Worker> {
        let config = Config {
            heartbeat_interval: env.heartbeat_interval,
        };

        let manager = Arc::new(TaskManager::new(
            &env.redis_url,
            "validity_prover",
            100, // dummy value
            (env.heartbeat_interval * 3) as usize,
        )?);
        let worker_id = Uuid::new_v4().to_string();
        Ok(Worker {
            config,
            transition_processor,
            manager,
            worker_id,
        })
    }

    async fn work(&self) -> Result<()> {
        loop {
            let task = self.manager.assign_task(&self.worker_id).await?;
            if task.is_none() {
                tokio::time::sleep(tokio::time::Duration::from_secs(TASK_POLLING_INTERVAL)).await;
                continue;
            }

            let (block_number, task) = task.unwrap();
            log::info!(
                "Processing block {} by worker {}",
                block_number,
                self.worker_id
            );

            // Prove the transition on another thread
            let transition_processor = self.transition_processor.clone();
            let TransitionProofTask {
                block_number: _,
                prev_validity_pis,
                validity_witness,
            } = task.clone();
            let result = tokio::task::spawn_blocking(move || {
                transition_processor.prove(&prev_validity_pis, &validity_witness)
            })
            .await
            .unwrap();

            let result = match result {
                Ok(proof) => {
                    log::info!(
                        "Proof generated for block_number {} by worker {}",
                        block_number,
                        self.worker_id
                    );
                    TransitionProofTaskResult {
                        block_number,
                        proof: Some(proof),
                        error: None,
                    }
                }
                Err(e) => {
                    log::error!("Error while proving: {:?} by worker {}", e, self.worker_id);
                    TransitionProofTaskResult {
                        block_number,
                        proof: None,
                        error: Some(e.to_string()),
                    }
                }
            };
            self.manager
                .complete_task(&self.worker_id, block_number, &task, &result)
                .await?;
        }
    }

    pub async fn run(&self) -> Vec<JoinHandle<()>> {
        let worker = self.clone();
        let solve_handle = tokio::spawn(async move {
            if let Err(e) = worker.work().await {
                eprintln!("Error: {:?}", e);
            }
        });
        let manager = self.manager.clone();
        let worker_id = self.worker_id.clone();
        let heartbeat_interval = self.config.heartbeat_interval;
        let submit_heartbeat_handle = tokio::spawn(async move {
            loop {
                log::info!("Submitting heartbeat for worker {}", worker_id);
                if let Err(e) = manager.submit_heartbeat(&worker_id).await {
                    eprintln!("Error: {:?}", e);
                }
                tokio::time::sleep(tokio::time::Duration::from_secs(heartbeat_interval)).await;
            }
        });
        log::info!("Starting worker with id {}", self.worker_id);
        vec![solve_handle, submit_heartbeat_handle]
    }
}
