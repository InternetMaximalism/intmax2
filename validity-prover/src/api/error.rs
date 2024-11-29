use intmax2_client_sdk::external_api::contract::interface::BlockchainError;
use intmax2_zkp::ethereum_types::bytes32::Bytes32;

#[derive(Debug, thiserror::Error)]
pub enum ObserverError {
    #[error("Blockchain error: {0}")]
    BlockchainError(#[from] BlockchainError),

    #[error("Database error: {0}")]
    DBError(#[from] sqlx::Error),

    #[error("Derialization error: {0}")]
    DeserializationError(#[from] serde_json::Error),

    #[error("Full block sync error: {0}")]
    FullBlockSyncError(String),

    #[error("Deposit sync error: {0}")]
    DepositSyncError(String),

    #[error("Block not found: {0}")]
    BlockNotFound(u32),

    #[error("Block number mismatch: {0} != {1}")]
    BlockNumberMismatch(u32, u32),
}

#[derive(Debug, thiserror::Error)]
pub enum ValidityProverError {
    #[error("Observer error: {0}")]
    ObserverError(#[from] ObserverError),

    #[error("Block witness generation error: {0}")]
    BlockWitnessGenerationError(String),

    #[error("Database error: {0}")]
    DBError(#[from] sqlx::Error),

    #[error("Derialization error: {0}")]
    DeserializationError(#[from] serde_json::Error),

    #[error("Failed to update trees: {0}")]
    FailedToUpdateTrees(String),

    #[error("Validity prove error: {0}")]
    ValidityProveError(String),

    #[error("Deposit tree root mismatch: expected {0}, got {1}")]
    DepositTreeRootMismatch(Bytes32, Bytes32),

    #[error("Validity proof not found for block number {0}")]
    ValidityProofNotFound(u32),

    #[error("Block tree not found for block number {0}")]
    BlockTreeNotFound(u32),

    #[error("Account tree not found for block number {0}")]
    AccountTreeNotFound(u32),

    #[error("Input error {0}")]
    InputError(String),
}
