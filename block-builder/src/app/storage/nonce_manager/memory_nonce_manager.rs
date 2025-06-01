use std::{collections::BTreeSet, sync::Arc};

use intmax2_client_sdk::external_api::contract::rollup_contract::RollupContract;
use tokio::sync::RwLock;
use tracing::instrument;

use super::{
    common::get_onchain_next_nonce, config::NonceManagerConfig, error::NonceError, NonceManager,
};

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

impl InMemoryNonceManager {
    async fn sync_onchain(&self) -> Result<(), NonceError> {
        let onchain_next_registration_nonce =
            get_onchain_next_nonce(&self.rollup, true, self.config.block_builder_address).await?;
        let onchain_next_non_registration_nonce =
            get_onchain_next_nonce(&self.rollup, false, self.config.block_builder_address).await?;

        let mut local_next_reg_guard = self.next_registration_nonce.write().await;
        *local_next_reg_guard = onchain_next_registration_nonce.max(*local_next_reg_guard);
        drop(local_next_reg_guard);

        let mut local_next_non_reg_guard = self.next_non_registration_nonce.write().await;
        *local_next_non_reg_guard =
            onchain_next_non_registration_nonce.max(*local_next_non_reg_guard);
        drop(local_next_non_reg_guard);

        let mut reserved_registration_nonces_guard =
            self.reserved_registration_nonces.write().await;
        reserved_registration_nonces_guard
            .retain(|&nonce| nonce >= onchain_next_registration_nonce);
        drop(reserved_registration_nonces_guard);

        let mut reserved_non_registration_nonces_guard =
            self.reserved_non_registration_nonces.write().await;
        reserved_non_registration_nonces_guard
            .retain(|&nonce| nonce >= onchain_next_non_registration_nonce);
        drop(reserved_non_registration_nonces_guard);

        Ok(())
    }
}

#[async_trait::async_trait(?Send)]
impl NonceManager for InMemoryNonceManager {
    #[instrument(skip(self))]
    async fn reserve_nonce(&self, is_registration: bool) -> Result<u32, NonceError> {
        // Synchronize the local state with the on-chain state.
        self.sync_onchain().await?;

        let mut next_nonce_guard = if is_registration {
            self.next_registration_nonce.write().await
        } else {
            self.next_non_registration_nonce.write().await
        };
        let next_nonce = *next_nonce_guard;
        *next_nonce_guard += 1;
        drop(next_nonce_guard);

        let reserved_nonces_arc = if is_registration {
            &self.reserved_registration_nonces
        } else {
            &self.reserved_non_registration_nonces
        };
        reserved_nonces_arc.write().await.insert(next_nonce);

        tracing::Span::current().record("next_nonce", next_nonce);
        Ok(next_nonce)
    }

    #[instrument(skip(self))]
    async fn release_nonce(&self, nonce: u32, is_registration: bool) -> Result<(), NonceError> {
        let reserved_nonces_arc = if is_registration {
            &self.reserved_registration_nonces
        } else {
            &self.reserved_non_registration_nonces
        };
        let mut reserved_nonces_set_guard = reserved_nonces_arc.write().await;
        reserved_nonces_set_guard.remove(&nonce);
        Ok(())
    }

    async fn smallest_reserved_nonce(
        &self,
        is_registration: bool,
    ) -> Result<Option<u32>, NonceError> {
        let reserved_nonces_guard = if is_registration {
            self.reserved_registration_nonces.read().await
        } else {
            self.reserved_non_registration_nonces.read().await
        };
        // `BTreeSet` iterators yield elements in ascending order.
        // So, the first element from `iter().next()` is the smallest.
        Ok(reserved_nonces_guard.iter().next().cloned())
    }
}
