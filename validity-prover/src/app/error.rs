use super::check_point_store::EventType;
use crate::trees::merkle_tree::error::MerkleTreeError;
use intmax2_client_sdk::external_api::contract::error::BlockchainError;
use intmax2_zkp::ethereum_types::{bytes32::Bytes32, EthereumTypeError};
use redis::RedisError;
use server_common::redis::task_manager::TaskManagerError;

#[derive(Debug, thiserror::Error)]
pub enum ObserverError {
    #[error("Blockchain error: {0}")]
    BlockchainError(#[from] BlockchainError),

    #[error("Leader election error: {0}")]
    LeaderError(#[from] LeaderError),

    #[error("Database error: {0}")]
    DBError(#[from] sqlx::Error),

    #[error("Deserialization error: {0}")]
    DeserializationError(#[from] bincode::Error),

    #[error("Event fetch error: {0}")]
    EventFetchError(String),

    #[error(
        "Event gap detected: {event_type} expected: {expected_next_event_id} < got: {got_event_id}"
    )]
    EventGapDetected {
        event_type: EventType,
        expected_next_event_id: u64,
        got_event_id: u64,
    },

    #[error("Ethereum type error: {0}")]
    EthereumTypeError(#[from] EthereumTypeError),

    #[error("Block not found: {0}")]
    BlockNotFound(u32),

    #[error("Block number mismatch: {0} != {1}")]
    BlockNumberMismatch(u32, u32),
}

#[derive(Debug, thiserror::Error)]
pub enum SettingConsistencyError {
    #[error("Database error: {0}")]
    DBError(#[from] sqlx::Error),

    #[error("Mismatched setting: {0}")]
    MismatchedSetting(String),
}

#[derive(Debug, thiserror::Error)]
pub enum LeaderError {
    #[error("Redis error: {0}")]
    RedisError(#[from] RedisError),

    #[error("Failed to acquire leader lock")]
    LockAcquisitionError,

    #[error("Failed to extend leader lock")]
    LockExtensionError,
}

#[derive(Debug, thiserror::Error)]
pub enum ValidityProverError {
    #[error("Observer error: {0}")]
    ObserverError(#[from] ObserverError),

    #[error("Leader election error: {0}")]
    LeaderError(#[from] LeaderError),

    #[error("Block witness generation error: {0}")]
    BlockWitnessGenerationError(String),

    #[error("Merkle tree error: {0}")]
    MerkleTreeError(#[from] MerkleTreeError),

    #[error("Task manager error: {0}")]
    TaskManagerError(#[from] TaskManagerError),

    #[error("Setting consistency error: {0}")]
    SettingConsistencyError(#[from] SettingConsistencyError),

    #[error("Task error: {0}")]
    TaskError(String),

    #[error("Database error: {0}")]
    DBError(#[from] sqlx::Error),

    #[error("Deserialization error: {0}")]
    DeserializationError(#[from] bincode::Error),

    #[error("Failed to update trees: {0}")]
    FailedToUpdateTrees(String),

    #[error("Validity prove error: {0}")]
    ValidityProveError(String),

    #[error("Failed to generate validity proof: {0}")]
    FailedToGenerateValidityProof(String),

    #[error("Deposit tree root mismatch: expected {0}, got {1}")]
    DepositTreeRootMismatch(Bytes32, Bytes32),

    #[error("Validity proof not found for block number {0}")]
    ValidityProofNotFound(u32),

    #[error("Block tree not found for block number {0}")]
    BlockTreeNotFound(u32),

    #[error("Account tree not found for block number {0}")]
    AccountTreeNotFound(u32),

    #[error("Deposit tree not found for block number {0}")]
    DepositTreeRootNotFound(u32),

    #[error("Validity witness not found for block number {0}")]
    ValidityWitnessNotFound(u32),

    #[error("Input error {0}")]
    InputError(String),
}
