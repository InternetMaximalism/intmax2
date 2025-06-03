use crate::app::error::FeeError;
use redis::RedisError as RedisClientError;
use serde_json::Error as SerdeJsonError;

use super::nonce_manager::error::NonceError;

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("Failed to add signature: {0}")]
    AddSignatureError(String),

    #[error("Failed query proposal: {0}")]
    QueryProposalError(String),

    #[error("Fee error: {0}")]
    FeeError(#[from] FeeError),

    #[error("Nonce error: {0}")]
    NonceError(#[from] NonceError),

    #[error("Redis error: {0}")]
    RedisError(#[from] RedisClientError),

    #[error("Serialization/Deserialization error: {0}")]
    SerdeError(#[from] SerdeJsonError),
}
