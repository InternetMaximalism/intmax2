use super::error::StoreVaultError;
use crate::EnvVar;
use intmax2_interfaces::{
    api::store_vault_server::{
        interface::{SaveDataEntry, MAX_BATCH_SIZE},
        types::{CursorOrder, DataWithMetaData, MetaDataCursor, MetaDataCursorResponse},
    },
    data::meta_data::MetaData,
    utils::digest::get_digest,
};
use intmax2_zkp::ethereum_types::{bytes32::Bytes32, u256::U256, u32limb_trait::U32LimbTrait};
use server_common::db::{DbPool, DbPoolConfig};

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

    async fn get_snapshot_digest(&self, topic: &str, pubkey: U256) -> Result<Option<Bytes32>> {
        let pubkey_hex = pubkey.to_hex();
        let record = sqlx::query!(
            r#"
            SELECT digest FROM s3_snapshot_data WHERE pubkey = $1 AND topic = $2
            "#,
            pubkey_hex,
            topic
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(record.map(|r| Bytes32::from_hex(&r.digest).unwrap()))
    }

    pub async fn save_snapshot_url(
        &self,
        topic: &str,
        pubkey: U256,
        prev_digest: Option<Bytes32>,
        new_digest: Bytes32,
    ) -> Result<String> {
        let current_digest = self.get_snapshot_digest(topic, pubkey).await?;
        // validation
        if let Some(prev_digest) = prev_digest {
            if let Some(digest) = current_digest {
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
        } else {
            return Err(StoreVaultError::LockError(
                "prev_digest not provided but data found".to_string(),
            ));
        }

        // insert new digest
        sqlx::query!(
            r#"
            INSERT INTO s3_snapshot_data (pubkey, topic, digest, upload_finished)
            VALUES ($1, $2, $3, false)
            ON CONFLICT (pubkey, topic) DO UPDATE SET digest = $3
            "#,
            pubkey.to_hex(),
            topic,
            new_digest.to_hex()
        )
        .execute(&self.pool)
        .await?;

        // publish s3 presigned url

        // check update upload_finish=true in other thread
        todo!()
    }

    pub async fn get_snapshot_url(&self, topic: &str, pubkey: U256) -> Result<Option<String>> {
        let digest = self.get_snapshot_digest(topic, pubkey).await?;
        if let Some(_digest) = digest {
            // todo: generate presigned url
            todo!()
        } else {
            Ok(None)
        }
    }

    pub async fn batch_save_data(&self, entries: &[SaveDataEntry]) -> Result<Vec<Bytes32>> {
        // Prepare values for bulk insert
        let topics: Vec<String> = entries.iter().map(|entry| entry.topic.clone()).collect();
        let pubkeys: Vec<String> = entries.iter().map(|entry| entry.pubkey.to_hex()).collect();
        let digests: Vec<Bytes32> = entries
            .iter()
            .map(|entry| get_digest(&entry.data))
            .collect();
        let digests_hex: Vec<String> = digests.iter().map(|d| d.to_hex()).collect();
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
            &digests_hex,
            &pubkeys,
            &topics,
            &data,
            &timestamps
        )
        .execute(&self.pool)
        .await?;

        Ok(digests)
    }

    pub async fn get_data_batch(
        &self,
        topic: &str,
        pubkey: U256,
        digests: &[Bytes32],
    ) -> Result<Vec<DataWithMetaData>> {
        let pubkey_hex = pubkey.to_hex();
        let digests_hex: Vec<String> = digests.iter().map(|d| d.to_hex()).collect();
        let records = sqlx::query!(
            r#"
            SELECT timestamp, digest
            FROM s3_historical_data
            WHERE topic = $1 AND pubkey = $2 AND digest = ANY($3)
            "#,
            topic,
            pubkey_hex,
            &digests_hex
        )
        .fetch_all(&self.pool)
        .await?;

        let meta: Vec<MetaData> = records
            .into_iter()
            .map(|r| MetaData {
                digest: Bytes32::from_hex(&r.digest).unwrap(),
                timestamp: r.timestamp as u64,
            })
            .collect();

        // generate presigned urls

        todo!()
    }

    pub async fn get_data_sequence_url(
        &self,
        topic: &str,
        pubkey: U256,
        cursor: &MetaDataCursor,
    ) -> Result<(Vec<String>, MetaDataCursorResponse)> {
        let pubkey_hex = pubkey.to_hex();
        let actual_limit = cursor.limit.unwrap_or(MAX_BATCH_SIZE as u32) as i64;

        let result: Vec<MetaData> = match cursor.order {
            CursorOrder::Asc => {
                let cursor_meta = cursor.cursor.clone().unwrap_or_default();
                sqlx::query!(
                    r#"
                    SELECT digest, timestamp
                    FROM s3_historical_data
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
                .map(|r| MetaData {
                    timestamp: r.timestamp as u64,
                    digest: Bytes32::from_hex(&r.digest).unwrap(),
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
                    SELECT digest, timestamp
                    FROM s3_historical_data
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
                .map(|r| MetaData {
                    digest: Bytes32::from_hex(&r.digest).unwrap(),
                    timestamp: r.timestamp as u64,
                })
                .collect()
            }
        };
        let has_more = result.len() > actual_limit as usize;
        let result = result
            .into_iter()
            .take(actual_limit as usize)
            .collect::<Vec<MetaData>>();
        let next_cursor = result.last().cloned();
        let total_count = sqlx::query_scalar!(
            r#"
            SELECT COUNT(*) FROM s3_historical_data
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

        // generate presigned urls

        todo!()
    }
}
