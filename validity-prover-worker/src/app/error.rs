use intmax2_interfaces::api::error::ServerError;
use server_common::redis::task_manager::TaskManagerError;

#[derive(Debug, thiserror::Error)]
pub enum WorkerError {
    #[error("Client error: {0}")]
    ClientError(#[from] ServerError),

    #[error("Transition prove failed: {0}")]
    TransitionProveFailed(String),

    #[error("Task manager error: {0}")]
    TaskManagerError(#[from] TaskManagerError),
}
