#[derive(Debug, Clone, thiserror::Error)]
pub enum IOError {
    #[error("Failed to create directory: {0}")]
    CreateDirAllError(String),
    #[error("Read error: {0}")]
    ReadError(String),
    #[error("Write error: {0}")]
    WriteError(String),
    #[error("Parse error: {0}")]
    ParseError(String),
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum LocalStoreVaultError {
    #[error(transparent)]
    IOError(#[from] IOError),

    #[error("Data not found error: {0}")]
    DataNotFoundError(String),

    #[error("Data inconsistency error: {0}")]
    DataInconsistencyError(String),
}
