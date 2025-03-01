use crate::app::error::FeeError;

#[derive(Debug, thiserror::Error)]
pub enum StateError {
    #[error("Failed to add signature: {0}")]
    AddSignatureError(String),

    #[error("Fee error: {0}")]
    FeeError(#[from] FeeError),
}
