use intmax2_client_sdk::external_api::contract::error::BlockchainError;
use intmax2_interfaces::api::error::ServerError;
use intmax2_zkp::ethereum_types::u256::U256;

#[derive(Debug, thiserror::Error)]
pub enum BlockBuilderError {
    #[error("Blockchain error: {0}")]
    BlockchainError(#[from] BlockchainError),

    #[error("Server error: {0}")]
    ServerError(#[from] ServerError),

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

    #[error("Unexpected error: {0}")]
    UnexpectedError(String),
}
