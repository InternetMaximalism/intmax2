use intmax2_client_sdk::external_api::contract::error::BlockchainError;
use intmax2_interfaces::api::error::ServerError;

#[derive(Debug, thiserror::Error)]
pub enum GraphClientError {
    #[error("Server error: {0}")]
    ServerError(#[from] ServerError),

    #[error("Blockchain error: {0}")]
    BlockchainError(#[from] BlockchainError),
}
