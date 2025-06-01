use error::NonceError;

pub mod config;
pub mod error;
pub mod memory_nonce_manager;

#[async_trait::async_trait(?Send)]
pub trait NonceManager: Sync + Send {
    /// Reserve a nonce for the current process. This should be used to ensure that the nonce is unique and not used by other processes.
    async fn reserve_nonce(&self, is_registration: bool) -> Result<u32, NonceError>;

    /// Release a previously reserved nonce. This should be called when the nonce is no longer needed.
    async fn release_nonce(&self, nonce: u32, is_registration: bool) -> Result<(), NonceError>;

    /// Checks if the given nonce is the smallest among all currently reserved nonces.
    async fn is_least_reserved_nonce(
        &self,
        nonce: u32,
        is_registration: bool,
    ) -> Result<bool, NonceError>;
}
