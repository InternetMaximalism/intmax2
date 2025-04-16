use std::{collections::HashSet, sync::Arc};

use intmax2_interfaces::api::validity_prover::interface::{
    TransitionProofTask, TransitionProofTaskResult,
};
use intmax2_zkp::circuits::validity::transition::processor::ValidityTransitionProcessor;
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
const RESTART_WAIT_INTERVAL: u64 = 30;

type Result<T> = std::result::Result<T, WorkerError>;

#[derive(Clone)]
struct Config {
    num_process: u32,
    heartbeat_interval: u64,
}

#[derive(Clone)]
pub struct Worker {
    config: Config,
    transition_processor: Arc<ValidityTransitionProcessor<F, C, D>>,
    manager: Arc<TaskManager<TransitionProofTask, TransitionProofTaskResult>>,
    worker_id: String,
    running_tasks: Arc<RwLock<HashSet<u32>>>,
}

impl Worker {
    pub fn new(
        env: &EnvVar,
        transition_processor: Arc<ValidityTransitionProcessor<F, C, D>>,
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
            .map_err(|e| format!("panic while proving: {:?}", e))
            .and_then(|r| r.map_err(|e| format!("error while proving: {:?}", e)));
            if let Err(e) = result {
                log::error!(
                    "error while proving for block number {}: {:?}",
                    block_number,
                    e
                );
                self.running_tasks.write().await.remove(&block_number);
                continue;
            }
            log::info!("proof generated for block_number {}", block_number,);
            let result = TransitionProofTaskResult {
                block_number,
                proof: result.ok(),
                error: None,
            };
            self.manager.complete_task(block_number, &result).await?;
            self.running_tasks.write().await.remove(&block_number);
            log::info!("completed block_number {}", block_number);
        }
    }

    async fn heartbeat(&self) -> Result<()> {
        loop {
            let running_tasks = self.running_tasks.read().await.clone();
            for block_number in running_tasks {
                if let Err(e) = self
                    .manager
                    .submit_heartbeat(&self.worker_id, block_number)
                    .await
                {
                    log::error!("error while submitting heartbeat: {:?}", e);
                } else {
                    log::info!("submitted heartbeat for block_number {}", block_number);
                }
            }
            tokio::time::sleep(tokio::time::Duration::from_secs(
                self.config.heartbeat_interval,
            ))
            .await;
        }
    }

    pub async fn run(&self) {
        for _ in 0..self.config.num_process {
            let worker = self.clone();
            tokio::spawn(async move {
                // restart loop
                loop {
                    if let Err(e) = worker.work().await {
                        eprintln!("Error: {:?}. Restarting", e);
                    }
                    tokio::time::sleep(tokio::time::Duration::from_secs(RESTART_WAIT_INTERVAL))
                        .await;
                }
            });
        }
        let worker = self.clone();
        tokio::spawn(async move {
            // restart loop
            loop {
                if let Err(e) = worker.heartbeat().await {
                    eprintln!("Error: {:?}. Restarting", e);
                }
                tokio::time::sleep(tokio::time::Duration::from_secs(RESTART_WAIT_INTERVAL)).await;
            }
        });
        log::info!("worker {} started", self.worker_id);
    }
}
