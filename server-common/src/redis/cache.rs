use redis::{aio::Connection, AsyncCommands as _, Client};
use serde::{de::DeserializeOwned, Serialize};
use std::time::Duration;

#[derive(thiserror::Error, Debug)]
pub enum RedisCacheError {
    #[error("Redis error: {0}")]
    RedisError(#[from] redis::RedisError),

    #[error("Serde error: {0}")]
    SerdeError(#[from] serde_json::Error),
}

pub struct RedisCache {
    client: Client,
    prefix: String,
}

impl RedisCache {
    pub fn new(redis_url: &str, prefix: &str) -> Result<Self, RedisCacheError> {
        let client = Client::open(redis_url)?;
        Ok(Self {
            client,
            prefix: prefix.to_owned(),
        })
    }

    async fn get_connection(&self) -> Result<Connection, RedisCacheError> {
        let conn = self.client.get_async_connection().await?;
        Ok(conn)
    }

    /// Get a value from a key
    pub async fn get<V>(&self, key: &str) -> Result<Option<V>, RedisCacheError>
    where
        V: DeserializeOwned,
    {
        let mut conn = self.get_connection().await?;
        let key = format!("{}:{}", self.prefix, key);

        // Check if the key exists
        let ttl: i64 = conn.ttl(&key).await?;
        if ttl < 0 {
            return Ok(None);
        }

        let result: Option<String> = conn.get(&key).await?;
        match result {
            Some(data) => {
                let value = serde_json::from_str(&data)?;
                Ok(Some(value))
            }
            None => Ok(None),
        }
    }

    /// Set a key with a value
    pub async fn set_with_ttl<V>(
        &self,
        key: &str,
        value: &V,
        ttl: Duration,
    ) -> Result<(), RedisCacheError>
    where
        V: Serialize + Send + Sync,
    {
        let mut conn = self.get_connection().await?;
        let serialized = serde_json::to_string(value)?;
        let key = format!("{}:{}", self.prefix, key);
        let () = conn.set_ex(key, serialized, ttl.as_secs() as usize).await?;
        Ok(())
    }

    /// Set a key with a value
    pub async fn delete(&self, key: &str) -> Result<bool, RedisCacheError> {
        let mut conn = self.get_connection().await?;
        let key = format!("{}:{}", self.prefix, key);
        let deleted: usize = conn.del(key).await?;
        Ok(deleted > 0)
    }

    /// Delete all keys with the current prefix
    pub async fn reset(&self) -> Result<(), RedisCacheError> {
        let mut conn = self.get_connection().await?;
        let pattern = format!("{}*", self.prefix);

        let keys: Vec<String> = conn.keys(&pattern).await?;

        if keys.is_empty() {
            return Ok(());
        }
        let _deleted: usize = conn.del(&keys).await?;
        Ok(())
    }

    /// Check if a key exists
    pub async fn exists(&self, key: &str) -> Result<bool, RedisCacheError> {
        let mut conn = self.get_connection().await?;
        let key = format!("{}:{}", self.prefix, key);
        let exists: bool = conn.exists(key).await?;
        Ok(exists)
    }
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use super::*;
    use std::{env, time::Duration};

    #[derive(Serialize, Deserialize, Debug)]
    pub struct TestStruct {
        pub name: String,
        pub age: u32,
    }

    #[tokio::test]
    #[ignore]
    async fn test_redis_cache() {
        let redis_url =
            env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".to_string());

        let cache = RedisCache::new(&redis_url, "test").unwrap();

        let key = "test_key";
        let value = TestStruct {
            name: "test".to_string(),
            age: 10,
        };

        cache
            .set_with_ttl(key, &value, Duration::from_secs(10))
            .await
            .unwrap();

        let result = cache.get::<TestStruct>(key).await.unwrap();
        assert_eq!(result.unwrap().name, value.name);
    }
}
