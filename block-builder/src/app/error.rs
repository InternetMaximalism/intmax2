use intmax2_client_sdk::{
    client::strategy::error::StrategyError, external_api::contract::error::BlockchainError,
};
use intmax2_interfaces::{
    api::error::ServerError,
    data::{encryption::errors::BlsEncryptionError, proof_compression::ProofCompressionError},
    utils::key::PublicKey,
};

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

    #[error("Invalid fee setting: {0}")]
    InvalidFeeSetting(String),

    #[error("Validity prover is not synced onchain:{0} validity prover:{1}")]
    ValidityProverIsNotSynced(u32, u32),

    #[error("Account already registered spend_pub: {0}, account_id: {1}")]
    AccountAlreadyRegistered(PublicKey, u64),

    #[error("Account not found for spend_pub: {0}")]
    AccountNotFound(PublicKey),

    #[error("Block already expired")]
    AlreadyExpired,

    #[error("Block chain health error: {0}")]
    BlockChainHealthError(String),
}

#[derive(Debug, thiserror::Error)]
pub enum FeeError {
    #[error("Fetch error: {0}")]
    FetchError(#[from] StrategyError),

    #[error("Proof compression error: {0}")]
    ProofCompressionError(#[from] ProofCompressionError),

    #[error("Server error: {0}")]
    ServerError(#[from] ServerError),

    #[error("Encryption error: {0}")]
    EncryptionError(#[from] BlsEncryptionError),

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
