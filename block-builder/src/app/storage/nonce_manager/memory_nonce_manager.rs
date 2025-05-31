use std::{collections::BTreeSet, sync::Arc};

use intmax2_client_sdk::external_api::contract::rollup_contract::RollupContract;
use tokio::sync::RwLock;

use super::{config::NonceManagerConfig, error::NonceError, NonceManager};

type AR<T> = Arc<RwLock<T>>;

#[derive(Debug, Clone)]
pub struct InMemoryNonceManager {
    pub config: NonceManagerConfig,
    pub rollup: RollupContract,
    pub next_registration_nonce: AR<u32>,
    pub next_non_registration_nonce: AR<u32>,
    pub reserved_registration_nonces: AR<BTreeSet<u32>>,
    pub reserved_non_registration_nonces: AR<BTreeSet<u32>>,
}

impl InMemoryNonceManager {
    pub fn new(config: NonceManagerConfig, rollup: RollupContract) -> Self {
        Self {
            config,
            rollup,
            next_registration_nonce: Arc::new(RwLock::new(0)),
            next_non_registration_nonce: Arc::new(RwLock::new(0)),
            reserved_registration_nonces: Arc::new(RwLock::new(BTreeSet::new())),
            reserved_non_registration_nonces: Arc::new(RwLock::new(BTreeSet::new())),
        }
    }
}

#[async_trait::async_trait(?Send)]
impl NonceManager for InMemoryNonceManager {
    async fn sync_onchain(&self) -> Result<(), NonceError> {
        let next_registration_nonce = self
            .rollup
            .get_nonce(true, self.config.block_builder_address)
            .await?;
        let next_non_registration_nonce = self
            .rollup
            .get_nonce(false, self.config.block_builder_address)
            .await?;

        *self.next_registration_nonce.write().await = next_registration_nonce;
        *self.next_non_registration_nonce.write().await = next_non_registration_nonce;

        // Clear all reservations older than the on-chain nonce
        let mut reserved_registration_nonces = self.reserved_registration_nonces.write().await;
        reserved_registration_nonces.retain(|&nonce| nonce >= next_registration_nonce);

        let mut reserved_non_registration_nonces =
            self.reserved_non_registration_nonces.write().await;
        reserved_non_registration_nonces.retain(|&nonce| nonce >= next_non_registration_nonce);
        Ok(())
    }

    async fn reserve_nonce(&self, is_registration: bool) -> Result<u32, NonceError> {
        let mut next_nonce = if is_registration {
            self.next_registration_nonce.write().await
        } else {
            self.next_non_registration_nonce.write().await
        };

        let reserved_nonces = if is_registration {
            &self.reserved_registration_nonces
        } else {
            &self.reserved_non_registration_nonces
        };

        // Find the next available nonce
        while reserved_nonces.read().await.contains(&*next_nonce) {
            *next_nonce += 1;
        }

        // Reserve the nonce
        if is_registration {
            self.reserved_registration_nonces
                .write()
                .await
                .insert(*next_nonce);
        } else {
            self.reserved_non_registration_nonces
                .write()
                .await
                .insert(*next_nonce);
        }
        Ok(*next_nonce)
    }

    async fn release_nonce(&self, nonce: u32, is_registration: bool) -> Result<(), NonceError> {
        let reserved_nonces = if is_registration {
            &self.reserved_registration_nonces
        } else {
            &self.reserved_non_registration_nonces
        };
        let mut reserved_nonces_set = reserved_nonces.write().await;
        reserved_nonces_set.remove(&nonce);
        Ok(())
    }

    async fn is_least_reserved_nonce(
        &self,
        nonce: u32,
        is_registration: bool,
    ) -> Result<bool, NonceError> {
        let reserved_nonces = if is_registration {
            self.reserved_registration_nonces.read().await
        } else {
            self.reserved_non_registration_nonces.read().await
        };
        // Check if the given nonce is the smallest among all currently reserved nonces
        Ok(reserved_nonces.iter().next() == Some(&nonce))
    }
}
