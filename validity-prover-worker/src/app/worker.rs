use std::{collections::HashSet, sync::Arc};

use intmax2_interfaces::api::validity_prover::interface::{
    TransitionProofTask, TransitionProofTaskResult,
};
use intmax2_zkp::circuits::validity::transition::processor::TransitionProcessor;
use plonky2::{field::goldilocks_field::GoldilocksField, plonk::config::PoseidonGoldilocksConfig};
use server_common::redis::task_manager::TaskManager;
use tokio::sync::RwLock;
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
    num_process: u32,
    heartbeat_interval: u64,
}

#[derive(Clone)]
pub struct Worker {
    config: Config,
    transition_processor: Arc<TransitionProcessor<F, C, D>>,
    manager: Arc<TaskManager<TransitionProofTask, TransitionProofTaskResult>>,
    worker_id: String,
    running_tasks: Arc<RwLock<HashSet<u32>>>,
}

impl Worker {
    pub fn new(
        env: &EnvVar,
        transition_processor: Arc<TransitionProcessor<F, C, D>>,
    ) -> Result<Worker> {
        let config = Config {
            num_process: env.num_process,
            heartbeat_interval: env.heartbeat_interval,
        };

        let manager = Arc::new(TaskManager::new(
            &env.redis_url,
            "validity_prover",
            env.task_ttl as usize,
            env.heartbeat_interval as usize,
        )?);
        let worker_id = Uuid::new_v4().to_string();
        Ok(Worker {
            config,
            transition_processor,
            manager,
            worker_id,
            running_tasks: Arc::new(RwLock::new(HashSet::new())),
        })
    }

    async fn work(&self) -> Result<()> {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(TASK_POLLING_INTERVAL)).await;

            let task = self.manager.assign_task().await?;
            if task.is_none() {
                continue;
            }

            let (block_number, task) = task.unwrap();
            self.running_tasks.write().await.insert(block_number);
            log::info!("processing block_number {}", block_number,);

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
                    log::info!("Proof generated for block_number {}", block_number,);
                    TransitionProofTaskResult {
                        block_number,
                        proof: Some(proof),
                        error: None,
                    }
                }
                Err(e) => {
                    log::error!(
                        "Error while proving for block number {}: {:?}",
                        block_number,
                        e,
                    );
                    TransitionProofTaskResult {
                        block_number,
                        proof: None,
                        error: Some(e.to_string()),
                    }
                }
            };
            self.manager.complete_task(block_number, &result).await?;
            self.running_tasks.write().await.remove(&block_number);

            log::info!("completed block_number {}", block_number);
        }
    }

    pub async fn run(&self) {
        for _ in 0..self.config.num_process {
            let worker = self.clone();
            tokio::spawn(async move {
                if let Err(e) = worker.work().await {
                    eprintln!("Error: {:?}", e);
                }
            });
        }
        log::info!("Worker started");

        let worker = self.clone();
        let worker_id = self.worker_id.clone();
        let heartbeat_interval = self.config.heartbeat_interval;
        tokio::spawn(async move {
            loop {
                let running_tasks = worker.running_tasks.read().await.clone();
                for block_number in running_tasks {
                    if let Err(e) = worker
                        .manager
                        .submit_heartbeat(&worker_id, block_number)
                        .await
                    {
                        log::error!("Error while submitting heartbeat: {:?}", e);
                    } else {
                        log::info!("submitted heartbeat for block_number {}", block_number);
                    }
                }
                tokio::time::sleep(tokio::time::Duration::from_secs(heartbeat_interval)).await;
            }
        });
    }
}
