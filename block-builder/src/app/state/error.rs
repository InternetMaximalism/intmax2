#[derive(Debug, thiserror::Error)]
pub enum StateError {
    #[error("Failed to add signature: {0}")]
    AddSignatureError(String),
}
