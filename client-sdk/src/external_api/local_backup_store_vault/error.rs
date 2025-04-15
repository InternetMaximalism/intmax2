#[derive(Debug, thiserror::Error)]
pub enum IOError {
    #[error("Failed to create directory: {0}")]
    CreateDirAllError(String),
    #[error("Read error: {0}")]
    ReadError(String),
    #[error("Write error: {0}")]
    WriteError(String),
    #[error("Delete error: {0}")]
    DeleteError(String),
    #[error("Parse error: {0}")]
    ParseError(String),
    #[error("Serialize error: {0}")]
    SerializeError(String),
}

#[derive(Debug, thiserror::Error)]
pub enum LocalStoreVaultError {
    #[error(transparent)]
    IOError(#[from] IOError),

    #[error("Data not found error: {0}")]
    DataNotFoundError(String),

    #[error("Data inconsistency error: {0}")]
    DataInconsistencyError(String),

    #[error("Lock error: {0}")]
    LockError(String),
}
