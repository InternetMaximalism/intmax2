use std::time::Duration;

use super::{
    error::StoreVaultError,
    s3::{S3Client, S3Config},
};
use crate::EnvVar;
use intmax2_interfaces::{
    api::{
        s3_store_vault::types::{PresignedUrlWithMetaData, S3SaveDataEntry},
        store_vault_server::{
            interface::MAX_BATCH_SIZE,
            types::{CursorOrder, MetaDataCursor, MetaDataCursorResponse},
        },
    },
    data::meta_data::MetaData,
};
use intmax2_zkp::ethereum_types::{bytes32::Bytes32, u256::U256, u32limb_trait::U32LimbTrait};
use server_common::db::{DbPool, DbPoolConfig};

// get path for s3 object
pub fn get_path(topic: &str, pubkey: U256, digest: Bytes32) -> String {
    format!("{}/{}/{}", topic, pubkey.to_hex(), digest.to_hex())
}

type Result<T> = std::result::Result<T, StoreVaultError>;

pub struct Config {
    pub s3_upload_timeout: u64,
    pub s3_download_timeout: u64,
}

pub struct S3StoreVault {
    config: Config,
    pool: DbPool,
    s3_client: S3Client,
}

impl S3StoreVault {
    pub async fn new(env: &EnvVar) -> Result<Self> {
        let pool = DbPool::from_config(&DbPoolConfig {
            max_connections: env.database_max_connections,
            idle_timeout: env.database_timeout,
            url: env.database_url.clone(),
        })
        .await?;
        let aws_config = aws_config::load_from_env().await;
        let s3_config = S3Config {
            bucket_name: env.bucket_name.clone(),
            cloudfront_domain: env.cloudfront_domain.clone(),
            cloudfront_key_pair_id: env.cloudfront_key_pair_id.clone(),
            private_key_path: env.private_key_path.clone(),
        };
        let s3_client = S3Client::new(aws_config, s3_config);

        let config = Config {
            s3_upload_timeout: env.s3_upload_timeout,
            s3_download_timeout: env.s3_download_timeout,
        };

        Ok(Self {
            config,
            pool,
            s3_client,
        })
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
        let path = get_path(topic, pubkey, new_digest);
        let presigned_url = self
            .s3_client
            .generate_presigned_upload_url(
                &path,
                "application/octet-stream",
                Duration::from_secs(self.config.s3_upload_timeout),
            )
            .await?;

        Ok(presigned_url)
    }

    pub async fn get_snapshot_url(&self, topic: &str, pubkey: U256) -> Result<Option<String>> {
        let digest = self.get_snapshot_digest(topic, pubkey).await?;
        if let Some(digest) = digest {
            let path = get_path(topic, pubkey, digest);
            let presigned_url = self
                .s3_client
                .generate_signed_url(&path, Duration::from_secs(self.config.s3_download_timeout))?;
            Ok(Some(presigned_url))
        } else {
            Ok(None)
        }
    }

    pub async fn batch_save_data_url(&self, entries: &[S3SaveDataEntry]) -> Result<Vec<String>> {
        // Prepare values for bulk insert
        let topics: Vec<String> = entries.iter().map(|entry| entry.topic.clone()).collect();
        let pubkeys: Vec<String> = entries.iter().map(|entry| entry.pubkey.to_hex()).collect();
        let digests: Vec<Bytes32> = entries.iter().map(|entry| entry.digest).collect();
        let digests_hex: Vec<String> = digests.iter().map(|d| d.to_hex()).collect();
        let timestamps = vec![chrono::Utc::now().timestamp(); entries.len()];
        let upload_finished = vec![false; entries.len()];

        sqlx::query!(
            r#"
            INSERT INTO s3_historical_data (digest, pubkey, topic, timestamp, upload_finished)
            SELECT
                UNNEST($1::text[]),
                UNNEST($2::text[]),
                UNNEST($3::text[]),
                UNNEST($4::bigint[]),
                UNNEST($5::bool[])
            ON CONFLICT (digest) DO NOTHING
            "#,
            &digests_hex,
            &pubkeys,
            &topics,
            &timestamps,
            &upload_finished
        )
        .execute(&self.pool)
        .await?;

        // generate presigned urls
        let mut presigned_urls = Vec::with_capacity(entries.len());
        for entry in entries {
            let path = get_path(&entry.topic, entry.pubkey, entry.digest);
            let presigned_url = self
                .s3_client
                .generate_presigned_upload_url(
                    &path,
                    "application/octet-stream",
                    Duration::from_secs(self.config.s3_upload_timeout),
                )
                .await?;
            presigned_urls.push(presigned_url);
        }

        Ok(presigned_urls)
    }

    pub async fn get_data_batch(
        &self,
        topic: &str,
        pubkey: U256,
        digests: &[Bytes32],
    ) -> Result<Vec<PresignedUrlWithMetaData>> {
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
        let mut url_with_meta = Vec::with_capacity(meta.len());
        for meta in meta.iter() {
            let path = get_path(topic, pubkey, meta.digest);
            let presigned_url = self
                .s3_client
                .generate_signed_url(&path, Duration::from_secs(self.config.s3_download_timeout))?;
            url_with_meta.push(PresignedUrlWithMetaData {
                presigned_url,
                meta: meta.clone(),
            });
        }

        Ok(url_with_meta)
    }

    pub async fn get_data_sequence_url(
        &self,
        topic: &str,
        pubkey: U256,
        cursor: &MetaDataCursor,
    ) -> Result<(Vec<PresignedUrlWithMetaData>, MetaDataCursorResponse)> {
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
        let mut presigned_urls_with_meta = Vec::with_capacity(result.len());
        for meta in result.iter() {
            let path = get_path(topic, pubkey, meta.digest);
            let presigned_url = self
                .s3_client
                .generate_signed_url(&path, Duration::from_secs(self.config.s3_download_timeout))?;
            presigned_urls_with_meta.push(PresignedUrlWithMetaData {
                presigned_url,
                meta: meta.clone(),
            });
        }

        Ok((presigned_urls_with_meta, response_cursor))
    }
}
