use super::s3::S3Error;

#[derive(Debug, thiserror::Error)]
pub enum StoreVaultError {
    #[error("Lock error: {0}")]
    LockError(String),

    #[error("Database error: {0}")]
    DBError(#[from] sqlx::Error),

    #[error("S3 error: {0}")]
    S3Error(#[from] S3Error),

    #[error("Deserialization error: {0}")]
    DeserializationError(#[from] bincode::Error),
}
