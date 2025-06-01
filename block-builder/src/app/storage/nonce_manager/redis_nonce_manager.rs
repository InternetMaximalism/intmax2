use super::{config::NonceManagerConfig, error::NonceError, NonceManager};
use intmax2_client_sdk::external_api::contract::rollup_contract::RollupContract;
use redis::{aio::ConnectionManager, AsyncCommands, Client};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::instrument;

pub struct RedisNonceManager {
    pub config: NonceManagerConfig,
    pub conn_manager: Arc<Mutex<ConnectionManager>>,
    pub rollup: RollupContract,

    pub next_registration_nonce_key: String,
    pub next_non_registration_nonce_key: String,
    pub reserved_registration_nonces_key: String,
    pub reserved_non_registration_nonces_key: String,
}

impl RedisNonceManager {
    pub async fn new(config: NonceManagerConfig, rollup: RollupContract) -> Self {
        let cluster_id = config
            .cluster_id
            .clone()
            .unwrap_or_else(|| "default".to_string());
        let prefix = format!("block_builder:{cluster_id}");

        let next_registration_nonce_key = format!("{prefix}:next_registration_nonce");
        let next_non_registration_nonce_key = format!("{prefix}:next_non_registration_nonce");
        let reserved_registration_nonces_key = format!("{prefix}:reserved_registration_nonces");
        let reserved_non_registration_nonces_key =
            format!("{prefix}:reserved_non_registration_nonces");

        let redis_url = config
            .redis_url
            .clone()
            .expect("redis_url not found in config");
        let client = Client::open(redis_url).expect("Failed to create Redis client");
        let conn_manager = ConnectionManager::new(client)
            .await
            .expect("Failed to create Redis connection manager");

        Self {
            config,
            conn_manager: Arc::new(Mutex::new(conn_manager)),
            rollup,
            next_registration_nonce_key,
            next_non_registration_nonce_key,
            reserved_registration_nonces_key,
            reserved_non_registration_nonces_key,
        }
    }

    async fn get_conn(&self) -> Result<ConnectionManager, NonceError> {
        let conn = self.conn_manager.lock().await;
        Ok(conn.clone())
    }

    #[instrument(skip(self))]
    async fn sync_onchain(&self) -> Result<(), NonceError> {
        let onchain_next_registration_nonce = self
            .rollup
            .get_nonce(true, self.config.block_builder_address)
            .await?;
        let onchain_next_non_registration_nonce = self
            .rollup
            .get_nonce(false, self.config.block_builder_address)
            .await?;

        let mut conn = self.get_conn().await?;

        // Sync registration nonces
        let local_next_reg_raw: Option<u32> = conn.get(&self.next_registration_nonce_key).await?;
        let local_next_reg = local_next_reg_raw.unwrap_or(0);
        let new_next_reg = onchain_next_registration_nonce.max(local_next_reg);
        conn.set(&self.next_registration_nonce_key, new_next_reg)
            .await?;

        let max_score_reg = onchain_next_registration_nonce as i64 - 1;
        conn.zremrangebyscore(&self.reserved_registration_nonces_key, 0, max_score_reg)
            .await?;

        // Sync non-registration nonces
        let local_next_non_reg_raw: Option<u32> =
            conn.get(&self.next_non_registration_nonce_key).await?;
        let local_next_non_reg = local_next_non_reg_raw.unwrap_or(0);
        let new_next_non_reg = onchain_next_non_registration_nonce.max(local_next_non_reg);
        conn.set(&self.next_non_registration_nonce_key, new_next_non_reg)
            .await?;

        let max_score_non_reg = onchain_next_non_registration_nonce as i64 - 1;
        conn.zremrangebyscore(
            &self.reserved_non_registration_nonces_key,
            0,
            max_score_non_reg,
        )
        .await?;

        Ok(())
    }
}

#[async_trait::async_trait(?Send)]
impl NonceManager for RedisNonceManager {
    async fn reserve_nonce(&self, is_registration: bool) -> Result<u32, NonceError> {
        self.sync_onchain().await?;

        let mut conn = self.get_conn().await?;

        let next_nonce_key = if is_registration {
            &self.next_registration_nonce_key
        } else {
            &self.next_non_registration_nonce_key
        };
        let reserved_nonces_key = if is_registration {
            &self.reserved_registration_nonces_key
        } else {
            &self.reserved_non_registration_nonces_key
        };

        let val_after_incr: i64 = conn.incr(next_nonce_key, 1).await?;
        let reserved_nonce = (val_after_incr - 1) as u32;

        conn.zadd(reserved_nonces_key, reserved_nonce, reserved_nonce)
            .await?;

        tracing::Span::current().record("next_nonce", reserved_nonce);
        Ok(reserved_nonce)
    }

    async fn release_nonce(&self, nonce: u32, is_registration: bool) -> Result<(), NonceError> {
        let mut conn = self.get_conn().await?;

        let reserved_nonces_key = if is_registration {
            &self.reserved_registration_nonces_key
        } else {
            &self.reserved_non_registration_nonces_key
        };

        conn.zrem(reserved_nonces_key, nonce).await?;

        Ok(())
    }

    async fn smallest_reserved_nonce(
        &self,
        is_registration: bool,
    ) -> Result<Option<u32>, NonceError> {
        let mut conn = self.get_conn().await?;

        let reserved_nonces_key = if is_registration {
            &self.reserved_registration_nonces_key
        } else {
            &self.reserved_non_registration_nonces_key
        };

        let result: Vec<u32> = conn.zrange(reserved_nonces_key, 0, 0).await?;

        Ok(result.first().cloned())
    }
}
