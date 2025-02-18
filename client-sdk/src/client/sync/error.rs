use intmax2_interfaces::{
    api::error::ServerError,
    data::{
        encryption::errors::BlsEncryptionError, error::DataError,
        proof_compression::ProofCompressionError,
    },
};
use intmax2_zkp::ethereum_types::bytes32::Bytes32;

use crate::{
    client::{receive_validation::ReceiveValidationError, strategy::error::StrategyError},
    external_api::contract::error::BlockchainError,
};

#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error("Server client error: {0}")]
    ServerError(#[from] ServerError),

    #[error("Strategy error: {0}")]
    StrategyError(#[from] StrategyError),

    #[error("Blockchain error: {0}")]
    BlockchainError(#[from] BlockchainError),

    #[error("Proof compression error: {0}")]
    ProofCompressionError(#[from] ProofCompressionError),

    #[error("Data error: {0}")]
    DataError(#[from] DataError),

    #[error("Encryption error: {0}")]
    EncryptionError(#[from] BlsEncryptionError),

    #[error("Receive validation error: {0}")]
    ReceiveValidationError(#[from] ReceiveValidationError),

    #[error("Fee error: {0}")]
    FeeError(String),

    #[error("Internal error: {0}")]
    InternalError(String),

    #[error("Decryption error: {0}")]
    DecryptionError(String),

    #[error("Pending withdrawal error: {0}")]
    PendingWithdrawalError(String),

    #[error("Witness generation error: {0}")]
    WitnessGenerationError(String),

    #[error("Failed to update private state: {0}")]
    FailedToUpdatePrivateState(String),

    #[error("Validity prover is not up to date: {0}")]
    ValidityProverIsNotSynced(String),

    #[error("Deposit info not found: {0}")]
    DepositInfoNotFound(Bytes32),

    #[error("Invalid transfer error: {0}")]
    InvalidTransferError(String),

    #[error("Block number mismatch balance_proof_block_number: {balance_proof_block_number} != block_number: {block_number}")]
    BalanceProofBlockNumberMismatch {
        balance_proof_block_number: u32,
        block_number: u32,
    },

    #[error("Balance proof not found")]
    BalanceProofNotFound,
}
