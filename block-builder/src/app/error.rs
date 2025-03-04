use intmax2_client_sdk::{
    client::strategy::error::StrategyError, external_api::contract::error::BlockchainError,
};
use intmax2_interfaces::{api::error::ServerError, data::proof_compression::ProofCompressionError};
use intmax2_zkp::ethereum_types::u256::U256;

use super::storage::error::StorageError;

#[derive(Debug, thiserror::Error)]
pub enum BlockBuilderError {
    #[error("Storage error: {0}")]
    StorageError(#[from] StorageError),

    #[error("Blockchain error: {0}")]
    BlockchainError(#[from] BlockchainError),

    #[error("Server error: {0}")]
    ServerError(#[from] ServerError),

    #[error("Fee error: {0}")]
    FeeError(#[from] FeeError),

    #[error("Queue error: {0}")]
    QueueError(String),

    #[error("Not accepting transactions")]
    NotAcceptingTx,

    #[error("Block is full")]
    BlockIsFull,

    #[error("Only one sender allowed in a block")]
    OnlyOneSenderAllowed,

    #[error("Validity prover is not synced onchain:{0} validity prover:{1}")]
    ValidityProverIsNotSynced(u32, u32),

    #[error("Account already registered pubkey: {0}, account_id: {1}")]
    AccountAlreadyRegistered(U256, u64),

    #[error("Account not found pubkey: {0}")]
    AccountNotFound(U256),

    #[error("Block builder is pausing")]
    BlockBuilderIsPausing,

    #[error("Not proposing")]
    NotProposing,

    #[error("Tx request is not found")]
    TxRequestNotFound,

    #[error("Invalid signature: {0}")]
    InvalidSignature(String),

    #[error("Block builder should be pausing")]
    ShouldBePausing,

    #[error("Block already expired")]
    AlreadyExpired,

    #[error("Unexpected error: {0}")]
    UnexpectedError(String),
}

#[derive(Debug, thiserror::Error)]
pub enum FeeError {
    #[error("Fetch error: {0}")]
    FetchError(#[from] StrategyError),

    #[error("Proof compression error: {0}")]
    ProofCompressionError(#[from] ProofCompressionError),

    #[error("Server error: {0}")]
    ServerError(#[from] ServerError),

    #[error("Fee verification error: {0}")]
    FeeVerificationError(String),

    #[error("Merkle tree error: {0}")]
    MerkleTreeError(String),

    #[error("Invalid recipient: {0}")]
    InvalidRecipient(String),

    #[error("Invalid fee: {0}")]
    InvalidFee(String),

    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Signature verification error: {0}")]
    SignatureVerificationError(String),
}
