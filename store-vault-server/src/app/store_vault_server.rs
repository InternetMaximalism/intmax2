use intmax2_interfaces::{
    api::store_vault_server::{
        interface::{SaveDataEntry, MAX_BATCH_SIZE},
        types::{
            CursorOrder, DataWithMetaData, DataWithTimestamp, MetaDataCursor,
            MetaDataCursorResponse,
        },
    },
    data::meta_data::MetaData,
    utils::digest::get_digest,
};
use intmax2_zkp::ethereum_types::{bytes32::Bytes32, u256::U256, u32limb_trait::U32LimbTrait};
use sqlx::Postgres;

use server_common::db::{DbPool, DbPoolConfig};

use crate::EnvVar;

use super::error::StoreVaultError;

type Result<T> = std::result::Result<T, StoreVaultError>;

pub struct StoreVaultServer {
    pool: DbPool,
}

impl StoreVaultServer {
    pub async fn new(env: &EnvVar) -> Result<Self> {
        let pool = DbPool::from_config(&DbPoolConfig {
            max_connections: env.database_max_connections,
            idle_timeout: env.database_timeout,
            url: env.database_url.clone(),
        })
        .await?;
        Ok(Self { pool })
    }

    pub async fn save_snapshot(
        &self,
        topic: &str,
        pubkey: U256,
        prev_digest: Option<Bytes32>,
        data: &[u8],
    ) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        let result = self.get_snapshot_and_digest(&mut tx, topic, pubkey).await?;
        // validation
        if let Some(prev_digest) = prev_digest {
            if let Some((_, digest)) = result {
                if digest != prev_digest {
                    return Err(StoreVaultError::LockError(format!(
                        "prev_digest {} mismatch with stored digest {}",
                        prev_digest, digest
                    )));
                }
            } else {
                return Err(StoreVaultError::LockError(
                    "prev_digest provided but no data found".to_string(),
                ));
            }
        } else if result.is_some() {
            return Err(StoreVaultError::LockError(
                "prev_digest not provided but data found".to_string(),
            ));
        }

        let pubkey_hex = pubkey.to_hex();
        let digest = get_digest(data);
        let digest_hex = digest.to_hex();
        sqlx::query!(
            r#"
            INSERT INTO snapshot_data (pubkey, digest, topic, data, timestamp)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (pubkey, topic) DO UPDATE SET data = EXCLUDED.data,
            digest = EXCLUDED.digest, timestamp = EXCLUDED.timestamp
            "#,
            pubkey_hex,
            digest_hex,
            topic,
            data,
            chrono::Utc::now().timestamp() as i64
        )
        .execute(tx.as_mut())
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn get_snapshot_data(&self, topic: &str, pubkey: U256) -> Result<Option<Vec<u8>>> {
        let mut tx = self.pool.begin().await?;
        let result = self.get_snapshot_and_digest(&mut tx, topic, pubkey).await?;
        tx.commit().await?;
        Ok(result.map(|(data, _)| data))
    }

    async fn get_snapshot_and_digest(
        &self,
        tx: &mut sqlx::Transaction<'_, Postgres>,
        topic: &str,
        pubkey: U256,
    ) -> Result<Option<(Vec<u8>, Bytes32)>> {
        let pubkey_hex = pubkey.to_hex();
        let record = sqlx::query!(
            r#"
            SELECT data, digest FROM snapshot_data WHERE pubkey = $1 AND topic = $2
            "#,
            pubkey_hex,
            topic
        )
        .fetch_optional(tx.as_mut())
        .await?;
        Ok(record.map(|r| (r.data, Bytes32::from_hex(&r.digest).unwrap())))
    }

    pub async fn batch_save_data(&self, entries: &[SaveDataEntry]) -> Result<()> {
        // Prepare values for bulk insert
        let topics: Vec<String> = entries.iter().map(|entry| entry.topic.clone()).collect();
        let pubkeys: Vec<String> = entries.iter().map(|entry| entry.pubkey.to_hex()).collect();
        let digests: Vec<String> = entries
            .iter()
            .map(|entry| get_digest(&entry.data).to_hex())
            .collect();
        let data: Vec<Vec<u8>> = entries.iter().map(|entry| entry.data.clone()).collect();
        let timestamps = vec![chrono::Utc::now().timestamp(); entries.len()];

        sqlx::query!(
            r#"
            INSERT INTO historical_data (digest, pubkey, topic, data, timestamp)
            SELECT
                UNNEST($1::text[]),
                UNNEST($2::text[]),
                UNNEST($3::text[]),
                UNNEST($4::bytea[]),
                UNNEST($5::bigint[])
            ON CONFLICT (digest) DO NOTHING
            "#,
            &digests,
            &pubkeys,
            &topics,
            &data,
            &timestamps
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_data_batch(
        &self,
        topic: &str,
        pubkey: U256,
        digests: &[Bytes32],
    ) -> Result<Vec<DataWithTimestamp>> {
        let pubkey_hex = pubkey.to_hex();
        let digests_hex: Vec<String> = digests.iter().map(|d| d.to_hex()).collect();
        let records = sqlx::query!(
            r#"
            SELECT data, timestamp
            FROM historical_data
            WHERE topic = $1 AND pubkey = $2 AND digest = ANY($3)
            "#,
            topic,
            pubkey_hex,
            &digests_hex
        )
        .fetch_all(&self.pool)
        .await?;

        let result: Vec<DataWithTimestamp> = records
            .into_iter()
            .map(|r| DataWithTimestamp {
                data: r.data,
                timestamp: r.timestamp as u64,
            })
            .collect();

        Ok(result)
    }

    pub async fn get_data_sequence(
        &self,
        topic: &str,
        pubkey: U256,
        cursor: &MetaDataCursor,
    ) -> Result<(Vec<DataWithMetaData>, MetaDataCursorResponse)> {
        let pubkey_hex = pubkey.to_hex();
        let actual_limit = cursor.limit.unwrap_or(MAX_BATCH_SIZE as u32) as i64;

        let result: Vec<DataWithMetaData> = match cursor.order {
            CursorOrder::Asc => {
                let cursor_meta = cursor.cursor.clone().unwrap_or_default();
                sqlx::query!(
                    r#"
    SELECT digest, data, timestamp
    FROM historical_data
    WHERE topic = $1
    AND pubkey = $2
    AND (timestamp > $3 OR (timestamp = $3 AND digest > $4))
    ORDER BY timestamp ASC, digest ASC
    LIMIT $5
    "#,
                    topic,
                    pubkey_hex,
                    cursor_meta.timestamp as i64,
                    cursor_meta.digest.to_hex(),
                    actual_limit + 1
                )
                .fetch_all(&self.pool)
                .await?
                .into_iter()
                .map(|r| {
                    let meta = MetaData {
                        timestamp: r.timestamp as u64,
                        digest: Bytes32::from_hex(&r.digest).unwrap(),
                    };
                    DataWithMetaData { meta, data: r.data }
                })
                .collect()
            }
            CursorOrder::Desc => {
                let (timestamp, digest) = cursor
                    .cursor
                    .as_ref()
                    .map(|meta| (meta.timestamp as i64, meta.digest.to_hex()))
                    .unwrap_or((i64::MAX, Bytes32::default().to_hex()));
                sqlx::query!(
                    r#"
                    SELECT digest, data, timestamp
                    FROM historical_data
                     WHERE topic = $1
                    AND pubkey = $2
                    AND (timestamp < $3 OR (timestamp = $3 AND digest < $4))
                    ORDER BY timestamp DESC, digest DESC
                    LIMIT $5
                "#,
                    topic,
                    pubkey_hex,
                    timestamp,
                    digest,
                    actual_limit + 1
                )
                .fetch_all(&self.pool)
                .await?
                .into_iter()
                .map(|r| {
                    let meta = MetaData {
                        digest: Bytes32::from_hex(&r.digest).unwrap(),
                        timestamp: r.timestamp as u64,
                    };
                    DataWithMetaData { meta, data: r.data }
                })
                .collect()
            }
        };
        let has_more = result.len() > actual_limit as usize;
        let result = result
            .into_iter()
            .take(actual_limit as usize)
            .collect::<Vec<DataWithMetaData>>();
        let next_cursor = result.last().map(|r| r.meta.clone());
        let total_count = sqlx::query_scalar!(
            r#"
            SELECT COUNT(*) FROM historical_data
            "#,
        )
        .fetch_one(&self.pool)
        .await?
        .unwrap_or(0) as u32;
        let response_cursor = MetaDataCursorResponse {
            next_cursor,
            has_more,
            total_count,
        };
        Ok((result, response_cursor))
    }
}

// #[cfg(test)]
// mod tests {
//     use ethers::core::rand;
//     use intmax2_interfaces::{
//         data::{encryption::BlsEncryption as _, meta_data::MetaData, snapshot_data::UserData},
//         utils::digest::get_digest,
//     };
//     use intmax2_zkp::common::signature::key_set::KeySet;

//     use crate::{app::store_vault_server::StoreVaultServer, EnvVar};

//     #[tokio::test]
//     async fn test_snapshot_data_get_and_save() -> anyhow::Result<()> {
//         dotenv::dotenv().ok();
//         let env: EnvVar = envy::from_env()?;
//         let store_vault_server = StoreVaultServer::new(&env).await?;
//         let mut rng = rand::thread_rng();
//         let key = KeySet::rand(&mut rng);
//         let encrypted_snapshot_data = store_vault_server.get_snapshot_data(key.pubkey).await?;
//         assert!(encrypted_snapshot_data.is_none());

//         let mut snapshot_data = UserData::new(key.pubkey);
//         let encrypted = snapshot_data.encrypt(key.pubkey);
//         let digest = get_digest(&encrypted);
//         store_vault_server
//             .save_snapshot_data(key.pubkey, None, &encrypted)
//             .await?;
//         let got_encrypted_snapshot_data = store_vault_server.get_snapshot_data(key.pubkey).await?;
//         assert_eq!(got_encrypted_snapshot_data.as_ref().unwrap(), &encrypted);
//         let digest2 = get_digest(&got_encrypted_snapshot_data.unwrap());
//         assert_eq!(digest, digest2);
//         snapshot_data.deposit_status.last_processed_meta_data = Some(MetaData {
//             timestamp: 1,
//             uuid: "test".to_string(),
//         });
//         let encrypted = snapshot_data.encrypt(key.pubkey);
//         let digest3 = get_digest(&encrypted);
//         store_vault_server
//             .save_snapshot_data(key.pubkey, Some(digest), &encrypted)
//             .await?;

//         snapshot_data.deposit_status.last_processed_meta_data = Some(MetaData {
//             timestamp: 2,
//             uuid: "test2".to_string(),
//         });
//         store_vault_server
//             .save_snapshot_data(key.pubkey, Some(digest3), &encrypted)
//             .await?;
//         Ok(())
//     }
// }
