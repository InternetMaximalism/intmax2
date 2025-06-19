use std::{collections::BTreeSet, sync::Arc};

use intmax2_client_sdk::external_api::contract::rollup_contract::RollupContract;
use tokio::sync::RwLock;
use tracing::instrument;

use super::{
    common::get_onchain_next_nonce, config::NonceManagerConfig, error::NonceError, NonceManager,
};

type AR<T> = Arc<RwLock<T>>;

#[derive(Debug, Clone)]
struct NonceState {
    next: AR<u32>,
    reserved: AR<BTreeSet<u32>>,
}

impl NonceState {
    pub fn new() -> Self {
        Self {
            next: Arc::new(RwLock::new(0)),
            reserved: Arc::new(RwLock::new(BTreeSet::new())),
        }
    }
}

#[derive(Debug, Clone)]
pub struct InMemoryNonceManager {
    config: NonceManagerConfig,
    rollup: RollupContract,
    registration: NonceState,
    non_registration: NonceState,
}

impl InMemoryNonceManager {
    pub fn new(config: NonceManagerConfig, rollup: RollupContract) -> Self {
        Self {
            config,
            rollup,
            registration: NonceState::new(),
            non_registration: NonceState::new(),
        }
    }

    async fn sync_onchain(&self) -> Result<(), NonceError> {
        let onchain_next_registration_nonce =
            get_onchain_next_nonce(&self.rollup, true, self.config.block_builder_address).await?;
        let onchain_next_non_registration_nonce =
            get_onchain_next_nonce(&self.rollup, false, self.config.block_builder_address).await?;

        {
            let mut local_next_reg_guard = self.registration.next.write().await;
            *local_next_reg_guard = onchain_next_registration_nonce.max(*local_next_reg_guard);
        }

        {
            let mut local_next_non_reg_guard = self.non_registration.next.write().await;
            *local_next_non_reg_guard =
                onchain_next_non_registration_nonce.max(*local_next_non_reg_guard);
        }

        {
            let mut reserved_registration_nonces_guard = self.registration.reserved.write().await;
            reserved_registration_nonces_guard
                .retain(|&nonce| nonce >= onchain_next_registration_nonce);
        }

        {
            let mut reserved_non_registration_nonces_guard =
                self.non_registration.reserved.write().await;
            reserved_non_registration_nonces_guard
                .retain(|&nonce| nonce >= onchain_next_non_registration_nonce);
        }

        Ok(())
    }

    fn get_reserved_nonces(&self, is_registration: bool) -> &AR<BTreeSet<u32>> {
        if is_registration {
            &self.registration.reserved
        } else {
            &self.non_registration.reserved
        }
    }

    fn get_next_nonce(&self, is_registration: bool) -> &AR<u32> {
        if is_registration {
            &self.registration.next
        } else {
            &self.non_registration.next
        }
    }
}

#[async_trait::async_trait(?Send)]
impl NonceManager for InMemoryNonceManager {
    #[instrument(skip(self))]
    async fn reserve_nonce(&self, is_registration: bool) -> Result<u32, NonceError> {
        // Synchronize the local state with the on-chain state.
        self.sync_onchain().await?;

        let mut next_nonce_guard = self.get_next_nonce(is_registration).write().await;
        let next_nonce = *next_nonce_guard;
        *next_nonce_guard += 1;
        drop(next_nonce_guard);

        let reserved_nonces_arc = self.get_reserved_nonces(is_registration);
        reserved_nonces_arc.write().await.insert(next_nonce);

        tracing::Span::current().record("next_nonce", next_nonce);
        Ok(next_nonce)
    }

    #[instrument(skip(self))]
    async fn release_nonce(&self, nonce: u32, is_registration: bool) -> Result<(), NonceError> {
        let reserved_nonces_arc = self.get_reserved_nonces(is_registration);
        let mut reserved_nonces_set_guard = reserved_nonces_arc.write().await;
        reserved_nonces_set_guard.remove(&nonce);
        Ok(())
    }

    async fn smallest_reserved_nonce(
        &self,
        is_registration: bool,
    ) -> Result<Option<u32>, NonceError> {
        let reserved_nonces_guard = self.get_reserved_nonces(is_registration).read().await;
        // `BTreeSet` iterators yield elements in ascending order.
        // So, the first element from `iter().next()` is the smallest.
        Ok(reserved_nonces_guard.iter().next().cloned())
    }
}
