use std::sync::Arc;

use intmax2_client_sdk::external_api::utils::time::sleep_for;
use intmax2_interfaces::api::validity_prover::interface::{
    TransitionProofTask, TransitionProofTaskResult,
};
use intmax2_zkp::circuits::validity::transition::processor::TransitionProcessor;
use plonky2::{field::goldilocks_field::GoldilocksField, plonk::config::PoseidonGoldilocksConfig};
use server_common::redis::task_manager::TaskManager;
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
    pub fn new(env: &EnvVar) -> Result<Worker> {
        let config = Config {
            heartbeat_interval: env.heartbeat_interval,
        };
        let transition_processor = Arc::new(TransitionProcessor::new());
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
                log::info!("No task assigned");
                sleep_for(TASK_POLLING_INTERVAL).await;
                continue;
            }

            let (block_number, task) = task.unwrap();
            log::info!("Processing task {}", block_number);

            let result = match self
                .transition_processor
                .prove(&task.prev_validity_pis, &task.validity_witness)
            {
                Ok(proof) => {
                    log::info!("Proof generated for block_number {}", block_number);
                    TransitionProofTaskResult {
                        block_number,
                        proof: Some(proof),
                        error: None,
                    }
                }
                Err(e) => {
                    log::error!("Error while proving: {:?}", e);
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

    pub async fn run(&self) {
        let worker = self.clone();
        let solve_handle = tokio::spawn(async move {
            log::info!("Starting worker");
            if let Err(e) = worker.work().await {
                eprintln!("Error: {:?}", e);
            }
        });

        let manager = self.manager.clone();
        let worker_id = self.worker_id.clone();
        let heartbeat_interval = self.config.heartbeat_interval;
        let submit_heartbeat_handle = tokio::spawn(async move {
            loop {
                log::info!("Submitting heartbeat");
                if let Err(e) = manager.submit_heartbeat(&worker_id).await {
                    eprintln!("Error: {:?}", e);
                }
                sleep_for(heartbeat_interval).await;
            }
        });
        tokio::try_join!(solve_handle, submit_heartbeat_handle).unwrap();
    }
}
