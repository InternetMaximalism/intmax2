use super::{
    common::get_onchain_next_nonce, config::NonceManagerConfig, error::NonceError, NonceManager,
};
use intmax2_client_sdk::external_api::{
    contract::rollup_contract::RollupContract, utils::retry::with_retry,
};
use redis::{aio::ConnectionManager, Client};
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

    async fn sync_onchain(&self) -> Result<(), NonceError> {
        let onchain_next_registration_nonce =
            get_onchain_next_nonce(&self.rollup, true, self.config.block_builder_address).await?;
        let onchain_next_non_registration_nonce =
            get_onchain_next_nonce(&self.rollup, false, self.config.block_builder_address).await?;

        with_retry(|| async {
            let mut conn = self.get_conn().await?;
            // Sync registration nonces
            let local_next_reg_raw: Option<u32> = redis::cmd("GET")
                .arg(&self.next_registration_nonce_key)
                .query_async(&mut conn)
                .await?;
            let local_next_reg = local_next_reg_raw.unwrap_or(0);
            let new_next_reg = onchain_next_registration_nonce.max(local_next_reg);
            let () = redis::cmd("SET")
                .arg(&self.next_registration_nonce_key)
                .arg(new_next_reg)
                .query_async(&mut conn)
                .await?;

            let max_score_reg = onchain_next_registration_nonce as i64 - 1;
            let () = redis::cmd("ZREMRANGEBYSCORE")
                .arg(&self.reserved_registration_nonces_key)
                .arg(0)
                .arg(max_score_reg)
                .query_async(&mut conn)
                .await?;

            // Sync non-registration nonces
            let local_next_non_reg_raw: Option<u32> = redis::cmd("GET")
                .arg(&self.next_non_registration_nonce_key)
                .query_async(&mut conn)
                .await?;
            let local_next_non_reg = local_next_non_reg_raw.unwrap_or(0);
            let new_next_non_reg = onchain_next_non_registration_nonce.max(local_next_non_reg);
            let () = redis::cmd("SET")
                .arg(&self.next_non_registration_nonce_key)
                .arg(new_next_non_reg)
                .query_async(&mut conn)
                .await?;

            let max_score_non_reg = onchain_next_non_registration_nonce as i64 - 1;
            let () = redis::cmd("ZREMRANGEBYSCORE")
                .arg(&self.reserved_non_registration_nonces_key)
                .arg(0)
                .arg(max_score_non_reg)
                .query_async(&mut conn)
                .await?;
            Ok(())
        })
        .await
    }
}

#[async_trait::async_trait(?Send)]
impl NonceManager for RedisNonceManager {
    async fn reserve_nonce(&self, is_registration: bool) -> Result<u32, NonceError> {
        self.sync_onchain().await?;

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

        let reserved_nonce = with_retry(|| async {
            let mut conn = self.get_conn().await?;
            let val_after_incr: i64 = redis::cmd("INCR")
                .arg(next_nonce_key)
                .query_async(&mut conn)
                .await?;
            let reserved_nonce = (val_after_incr - 1) as u32;
            let _: i64 = redis::cmd("ZADD")
                .arg(reserved_nonces_key)
                .arg(reserved_nonce)
                .arg(reserved_nonce)
                .query_async(&mut conn)
                .await?;
            Result::<_, NonceError>::Ok(reserved_nonce)
        })
        .await?;
        log::info!(
            "Reserved nonce: {} for {}",
            reserved_nonce,
            if is_registration {
                "registration"
            } else {
                "non-registration"
            }
        );
        Ok(reserved_nonce)
    }

    #[instrument(skip(self))]
    async fn release_nonce(&self, nonce: u32, is_registration: bool) -> Result<(), NonceError> {
        with_retry(|| async {
            let mut conn = self.get_conn().await?;
            let reserved_nonces_key = if is_registration {
                &self.reserved_registration_nonces_key
            } else {
                &self.reserved_non_registration_nonces_key
            };
            let () = redis::cmd("ZREM")
                .arg(reserved_nonces_key)
                .arg(nonce)
                .query_async(&mut conn)
                .await?;

            Ok(())
        })
        .await
    }

    #[instrument(skip(self))]
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

        let result: Vec<u32> = redis::cmd("ZRANGE")
            .arg(reserved_nonces_key)
            .arg(0)
            .arg(0)
            .query_async(&mut conn)
            .await?;

        Ok(result.first().cloned())
    }
}

#[cfg(test)]
mod tests {
    use crate::app::storage::redis_storage::test_redis_helper::{
        find_free_port, run_redis_docker, stop_redis_docker,
    };

    use super::*;
    use alloy::{
        primitives::Address,
        providers::{mock::Asserter, ProviderBuilder},
        sol_types::SolCall as _,
    };
    use intmax2_client_sdk::external_api::contract::rollup_contract::Rollup;
    use std::str::FromStr;

    fn set_reg_nonce_asserter(asserter: &Asserter, nonce: u32) {
        let reg_nonce_return = Rollup::builderRegistrationNonceCall::abi_encode_returns(&nonce);
        asserter.push_success(&reg_nonce_return);
    }

    fn set_non_reg_nonce_asserter(asserter: &Asserter, nonce: u32) {
        let non_reg_nonce_return =
            Rollup::builderNonRegistrationNonceCall::abi_encode_returns(&nonce);
        asserter.push_success(&non_reg_nonce_return);
    }

    async fn create_client(port: u16) -> (RedisNonceManager, Asserter) {
        let config = NonceManagerConfig {
            block_builder_address: Address::from_str("0x1234567890123456789012345678901234567890")
                .unwrap(),
            redis_url: Some(format!("redis://localhost:{port}")),
            cluster_id: Some("test".to_string()),
        };
        let asserter = Asserter::new();
        let provider = ProviderBuilder::default()
            .with_gas_estimation()
            .with_simple_nonce_management()
            .fetch_chain_id()
            .connect_mocked_client(asserter.clone());
        let rollup = RollupContract::new(provider, Default::default());
        (RedisNonceManager::new(config, rollup).await, asserter)
    }

    #[tokio::test]
    async fn test_nonce_manager_reserve_and_release() {
        let port = find_free_port();
        let cont_name = "redis-test_nonce_manager_reserve_and_release";

        stop_redis_docker(cont_name);
        let output = run_redis_docker(port, cont_name);
        assert!(
            output.status.success(),
            "Couldn't start {}: {}",
            cont_name,
            String::from_utf8_lossy(&output.stderr)
        );

        let (client, asserter) = create_client(port).await;

        // sync onchain nonces
        set_reg_nonce_asserter(&asserter, 10);
        set_non_reg_nonce_asserter(&asserter, 20);

        let reg_nonce = client.reserve_nonce(true).await.unwrap();
        assert_eq!(reg_nonce, 10);

        set_reg_nonce_asserter(&asserter, 10);
        set_non_reg_nonce_asserter(&asserter, 20);
        let non_reg_nonce = client.reserve_nonce(false).await.unwrap();
        assert_eq!(non_reg_nonce, 20);

        set_reg_nonce_asserter(&asserter, 10);
        set_non_reg_nonce_asserter(&asserter, 20);
        let reg_nonce2 = client.reserve_nonce(true).await.unwrap();
        assert_eq!(reg_nonce2, 11);

        set_reg_nonce_asserter(&asserter, 10);
        set_non_reg_nonce_asserter(&asserter, 20);
        let non_reg_nonce2 = client.reserve_nonce(false).await.unwrap();
        assert_eq!(non_reg_nonce2, 21);

        let smallest_reg_nonce = client.smallest_reserved_nonce(true).await.unwrap();
        assert_eq!(smallest_reg_nonce, Some(10));

        let smallest_non_reg_nonce = client.smallest_reserved_nonce(false).await.unwrap();
        assert_eq!(smallest_non_reg_nonce, Some(20));

        client.release_nonce(10, true).await.unwrap();
        let smallest_reg_nonce_after_release = client.smallest_reserved_nonce(true).await.unwrap();
        assert_eq!(smallest_reg_nonce_after_release, Some(11));

        client.release_nonce(20, false).await.unwrap();
        let smallest_non_reg_nonce_after_release =
            client.smallest_reserved_nonce(false).await.unwrap();
        assert_eq!(smallest_non_reg_nonce_after_release, Some(21));
    }

    #[tokio::test]
    async fn test_nonce_manager_clean_when_sync() {
        let port = find_free_port();
        let cont_name = "redis-test_nonce_manager_clean_when_sync";

        stop_redis_docker(cont_name);
        let output = run_redis_docker(port, cont_name);
        assert!(
            output.status.success(),
            "Couldn't start {}: {}",
            cont_name,
            String::from_utf8_lossy(&output.stderr)
        );

        let (client, asserter) = create_client(port).await;

        set_reg_nonce_asserter(&asserter, 10);
        set_non_reg_nonce_asserter(&asserter, 20);
        let nonce1 = client.reserve_nonce(true).await.unwrap();
        assert_eq!(nonce1, 10);

        set_reg_nonce_asserter(&asserter, 10);
        set_non_reg_nonce_asserter(&asserter, 20);
        let nonce2 = client.reserve_nonce(true).await.unwrap();
        assert_eq!(nonce2, 11);

        set_reg_nonce_asserter(&asserter, 10);
        set_non_reg_nonce_asserter(&asserter, 20);
        let nonce3 = client.reserve_nonce(true).await.unwrap();
        assert_eq!(nonce3, 12);

        set_reg_nonce_asserter(&asserter, 11);
        set_non_reg_nonce_asserter(&asserter, 20);
        client.sync_onchain().await.unwrap();
        let smallest_reg_nonce = client.smallest_reserved_nonce(true).await.unwrap();
        assert_eq!(smallest_reg_nonce, Some(11));
    }
}
