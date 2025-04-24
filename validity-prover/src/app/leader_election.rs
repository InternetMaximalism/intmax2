use redis::{Client, Script};
use std::{sync::Arc, time::Duration};
use uuid::Uuid;

use super::error::LeaderError;

#[derive(Clone, Debug)]
pub struct LeaderElection {
    client: Client,
    node_id: String,
    lock_key: String,
    lock_ttl: Duration,
}

impl LeaderElection {
    pub fn new(
        redis_url: &str,
        lock_key: impl Into<String>,
        lock_ttl: Duration,
    ) -> Result<Self, LeaderError> {
        let client = Client::open(redis_url)?;
        let node_id = Uuid::new_v4().to_string();
        tracing::info!("Node ID = {node_id}");
        Ok(Self {
            client,
            node_id,
            lock_key: lock_key.into(),
            lock_ttl,
        })
    }

    async fn try_acquire_leadership(&self) -> Result<bool, LeaderError> {
        static ACQUIRE_OR_REFRESH: &str = r#"
            local val = redis.call('GET', KEYS[1])
            if not val then
                -- no lock yet → create it
                return redis.call('SET', KEYS[1], ARGV[1], 'PX', ARGV[2], 'NX') and 1 or 0
            elseif val == ARGV[1] then
                -- already my lock → just refresh TTL
                return redis.call('PEXPIRE', KEYS[1], ARGV[2])
            else
                -- someone else's lock
                return 0
            end
        "#;
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        // 1 = success, 0 = someone else is leader
        let ok: i32 = Script::new(ACQUIRE_OR_REFRESH)
            .key(&self.lock_key)
            .arg(&self.node_id)
            .arg(self.lock_ttl.as_millis() as usize)
            .invoke_async(&mut conn)
            .await?;
        Ok(ok == 1)
    }

    #[tracing::instrument(skip(self))]
    pub async fn wait_for_leadership(&self) -> Result<(), LeaderError> {
        loop {
            if self.try_acquire_leadership().await? {
                return Ok(());
            }
            tracing::warn!("waiting for leadership...");
            tokio::time::sleep(self.lock_ttl).await;
        }
    }

    async fn extend_leadership_loop(self: Arc<Self>) -> Result<(), LeaderError> {
        let mut interval = tokio::time::interval(self.lock_ttl / 3);
        loop {
            interval.tick().await;
            if !self.try_acquire_leadership().await? {
                tracing::warn!("lost leadership");
            }
        }
    }

    #[tracing::instrument(skip(self))]
    pub fn start_job(&self) {
        let self_clone = Arc::new(self.clone());
        tokio::spawn(async move {
            loop {
                let lock_ttl = self_clone.lock_ttl;
                let self_clone = self_clone.clone();
                let handler =
                    tokio::spawn(async move { self_clone.extend_leadership_loop().await });
                match handler.await {
                    Ok(Ok(())) => {
                        tracing::error!("Leadership extended finished");
                    }
                    Ok(Err(e)) => {
                        tracing::error!("Error extending leadership: {e}");
                    }
                    Err(e) => {
                        tracing::error!("Panic extending leadership: {e}");
                    }
                }
                tokio::time::sleep(lock_ttl).await;
            }
        });
    }
}
