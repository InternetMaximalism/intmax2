use error::NonceError;

pub mod error;

#[async_trait::async_trait(?Send)]
pub trait NonceManager: Sync + Send {
    /// Synchronize the nonce with on-chain data. All reservations older than the on-chain nonce will be cleared.
    async fn sync_onchain(&self) -> Result<(), NonceError>;

    /// Reserve a nonce for the current process. This should be used to ensure that the nonce is unique and not used by other processes.
    async fn reserve_nonce(&self) -> Result<u32, NonceError>;

    /// Release a previously reserved nonce. This should be called when the nonce is no longer needed.
    async fn release_nonce(&self, nonce: u32) -> Result<(), NonceError>;

    /// Checks if the given nonce is the smallest among all currently reserved nonces.
    async fn is_least_reserved_nonce(&self, nonce: u32) -> Result<bool, NonceError>;
}
