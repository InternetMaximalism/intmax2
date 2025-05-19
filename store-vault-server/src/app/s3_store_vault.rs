use std::time::Duration;

use super::{error::StoreVaultError, s3::S3Config};
use crate::EnvVar;
use aws_config::BehaviorVersion;
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

#[cfg(test)]
use crate::app::s3::MockS3Client as S3Client;
#[cfg(not(test))]
use crate::app::s3::S3Client;

// get path for s3 object
pub fn get_path(topic: &str, pubkey: U256, digest: Bytes32) -> String {
    format!("{}/{}/{}", topic, pubkey.to_hex(), digest.to_hex())
}

type Result<T> = std::result::Result<T, StoreVaultError>;

#[derive(Clone)]
pub struct Config {
    pub s3_upload_timeout: u64,
    pub s3_download_timeout: u64,
    pub cleanup_interval: u64,
}

#[derive(Clone)]
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
        let aws_config = aws_config::load_defaults(BehaviorVersion::latest()).await;
        let s3_config = S3Config {
            bucket_name: env.bucket_name.clone(),
            cloudfront_domain: env.cloudfront_domain.clone(),
            cloudfront_key_pair_id: env.cloudfront_key_pair_id.clone(),
            cloudfront_private_key_base64: env.cloudfront_private_key_base64.clone(),
        };
        let s3_client = S3Client::new(aws_config, s3_config);

        let config = Config {
            s3_upload_timeout: env.s3_upload_timeout,
            s3_download_timeout: env.s3_download_timeout,
            cleanup_interval: env.cleanup_interval,
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

    pub async fn pre_save_snapshot(
        &self,
        topic: &str,
        pubkey: U256,
        digest: Bytes32,
    ) -> Result<String> {
        let path = get_path(topic, pubkey, digest);
        let presigned_url = self
            .s3_client
            .generate_upload_url(
                &path,
                "application/octet-stream",
                Duration::from_secs(self.config.s3_upload_timeout),
            )
            .await?;

        // save pending upload to db
        sqlx::query!(
            r#"
            INSERT INTO s3_snapshot_pending_uploads (digest, pubkey, topic, timestamp)
            VALUES ($1, $2, $3, $4)
            "#,
            digest.to_hex(),
            pubkey.to_hex(),
            topic,
            chrono::Utc::now().timestamp() as i64
        )
        .execute(&self.pool)
        .await?;

        Ok(presigned_url)
    }

    pub async fn save_snapshot(
        &self,
        topic: &str,
        pubkey: U256,
        prev_digest: Option<Bytes32>,
        digest: Bytes32,
    ) -> Result<()> {
        let current_digest = self.get_snapshot_digest(topic, pubkey).await?;
        // validation
        if current_digest != prev_digest {
            return Err(StoreVaultError::LockError(format!(
                "prev_digest mismatch with stored digest: {:?}",
                current_digest
            )));
        }

        // check timestamp
        let record = sqlx::query!(
            r#"
            SELECT timestamp FROM s3_snapshot_pending_uploads WHERE digest = $1
            "#,
            digest.to_hex(),
        )
        .fetch_optional(&self.pool)
        .await?;
        if let Some(record) = record {
            if record.timestamp as u64 + 2 * self.config.s3_upload_timeout
                < chrono::Utc::now().timestamp() as u64
            {
                return Err(StoreVaultError::ValidationError(
                    "it took too much time after pre_save_snapshot".to_string(),
                ));
            }
        } else {
            return Err(StoreVaultError::ValidationError(
                "pre_save_snapshot should be called before".to_string(),
            ));
        }

        let new_path = get_path(topic, pubkey, digest);
        if !self.s3_client.check_object_exists(&new_path).await? {
            return Err(StoreVaultError::ObjectError(format!(
                "object {} doesn't exist",
                new_path
            )));
        }

        let mut tx = self.pool.begin().await?;
        // insert new digest
        sqlx::query!(
            r#"
            INSERT INTO s3_snapshot_data (pubkey, topic, digest, timestamp)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (pubkey, topic) DO UPDATE SET digest = EXCLUDED.digest,
            timestamp = EXCLUDED.timestamp
            "#,
            pubkey.to_hex(),
            topic,
            digest.to_hex(),
            chrono::Utc::now().timestamp() as i64
        )
        .execute(tx.as_mut())
        .await?;

        // delete pending upload
        sqlx::query!(
            r#"
            DELETE FROM s3_snapshot_pending_uploads WHERE pubkey = $1 AND topic = $2 AND digest = $3
            "#,
            pubkey.to_hex(),
            topic,
            digest.to_hex()
        )
        .execute(tx.as_mut())
        .await?;
        tx.commit().await?;

        // delete old data if it exists
        if let Some(prev_digest) = prev_digest {
            let prev_path = get_path(topic, pubkey, prev_digest);
            self.s3_client.delete_object(&prev_path).await?;
        }

        Ok(())
    }

    pub async fn get_snapshot_url(&self, topic: &str, pubkey: U256) -> Result<Option<String>> {
        let digest = self.get_snapshot_digest(topic, pubkey).await?;
        if let Some(digest) = digest {
            let path = get_path(topic, pubkey, digest);
            let presigned_url = self.s3_client.generate_download_url(
                &path,
                Duration::from_secs(self.config.s3_download_timeout),
            )?;
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

        let result = sqlx::query!(
            r#"
            INSERT INTO s3_historical_data (digest, pubkey, topic, timestamp, upload_finished)
            SELECT
                UNNEST($1::text[]),
                UNNEST($2::text[]),
                UNNEST($3::text[]),
                UNNEST($4::bigint[]),
                UNNEST($5::bool[])
            "#,
            &digests_hex,
            &pubkeys,
            &topics,
            &timestamps,
            &upload_finished
        )
        .execute(&self.pool)
        .await;

        match result {
            Ok(_) => {}
            Err(sqlx::Error::Database(error))
                if error.constraint() == Some("s3_historical_data_pkey") =>
            {
                return Err(StoreVaultError::SaveHistoryError(
                    "data with the specified digest already in history".to_owned(),
                ))
            }
            Err(err) => return Err(err.into()),
        }

        // generate presigned urls
        let mut presigned_urls = Vec::with_capacity(entries.len());
        for entry in entries {
            let path = get_path(&entry.topic, entry.pubkey, entry.digest);
            let presigned_url = self
                .s3_client
                .generate_upload_url(
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
            let presigned_url = self.s3_client.generate_download_url(
                &path,
                Duration::from_secs(self.config.s3_download_timeout),
            )?;

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
            let presigned_url = self.s3_client.generate_download_url(
                &path,
                Duration::from_secs(self.config.s3_download_timeout),
            )?;
            presigned_urls_with_meta.push(PresignedUrlWithMetaData {
                presigned_url,
                meta: meta.clone(),
            });
        }

        Ok((presigned_urls_with_meta, response_cursor))
    }

    // Fetch unfinished data and check if they exist in s3.
    // If they exist, set upload_finished to true. If they are timed out, delete them.
    async fn cleanup_historical_data(&self) -> Result<()> {
        let records = sqlx::query!(
            r#"
            SELECT topic, pubkey, digest, timestamp
            FROM s3_historical_data
            WHERE upload_finished = false
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        let current_time = chrono::Utc::now().timestamp() as u64;
        for record in records {
            let path = get_path(
                &record.topic,
                U256::from_hex(&record.pubkey).unwrap(),
                Bytes32::from_hex(&record.digest).unwrap(),
            );
            let exists = self.s3_client.check_object_exists(&path).await?;
            if exists {
                sqlx::query!(
                    r#"
                    UPDATE s3_historical_data
                    SET upload_finished = true
                    WHERE digest = $1
                    "#,
                    record.digest
                )
                .execute(&self.pool)
                .await?;
            } else if self.config.s3_upload_timeout + (record.timestamp as u64) < current_time {
                sqlx::query!(
                    r#"
                    DELETE FROM s3_historical_data
                    WHERE digest = $1
                    "#,
                    record.digest
                )
                .execute(&self.pool)
                .await?;
                log::warn!("Historical data not found in s3. Deleted: path={}", path);
            }
        }
        Ok(())
    }

    // s3_snapshot_pending_uploads is deleted when saved in save_snapshot.
    // If s3_snapshot_pending_uploads remains, it is saved in s3, but not saved in DB, and is dangling.
    // This function cleans up such data.
    async fn cleanup_snapshot_data(&self) -> Result<()> {
        // get all pending uploads
        let records = sqlx::query!(
            r#"
            SELECT topic, pubkey, digest, timestamp
            FROM s3_snapshot_pending_uploads
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        let current_time = chrono::Utc::now().timestamp() as u64;
        for record in records {
            let path = get_path(
                &record.topic,
                U256::from_hex(&record.pubkey).unwrap(),
                Bytes32::from_hex(&record.digest).unwrap(),
            );
            if self.config.s3_upload_timeout * 4 + (record.timestamp as u64) < current_time {
                self.s3_client.delete_object(&path).await?;
                sqlx::query!(
                    r#"
                    DELETE FROM s3_snapshot_pending_uploads
                    WHERE digest = $1
                    "#,
                    record.digest
                )
                .execute(&self.pool)
                .await?;
                log::warn!("Pending upload not found in s3. Deleted: path={}", path);
            }
        }
        Ok(())
    }

    pub fn run(&self) {
        let period = Duration::from_secs(self.config.cleanup_interval);
        let self_clone = self.clone();
        actix_web::rt::spawn(async move {
            let mut interval = tokio::time::interval(period);
            loop {
                interval.tick().await;
                if let Err(e) = self_clone.cleanup_historical_data().await {
                    log::error!("Error in cleanup_historical_data: {:?}", e);
                }
            }
        });

        let self_clone = self.clone();
        actix_web::rt::spawn(async move {
            let mut interval = tokio::time::interval(period);
            loop {
                interval.tick().await;
                if let Err(e) = self_clone.cleanup_snapshot_data().await {
                    log::error!("Error in cleanup_snapshot_data: {:?}", e);
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use intmax2_interfaces::utils::digest::get_digest;
    use intmax2_zkp::ethereum_types::u256::U256;
    use mockall::predicate::eq;
    use sqlx::{Executor, PgPool, Postgres};
    use tokio::time::sleep;

    use super::*;

    #[tokio::test]
    #[ignore]
    async fn update_snapshot_test() {
        let _ = env_logger::builder().is_test(true).try_init();
        dotenvy::dotenv().ok();

        let env = envy::from_env::<crate::EnvVar>().unwrap();
        let s3_store_vault = S3StoreVault::new(&env).await.unwrap();

        let topic = "test-2";
        let pubkey = U256::from(1);
        let data = b"test data 1";

        let prev_digest = None;
        let new_digest = get_digest(data);
        let url = s3_store_vault
            .pre_save_snapshot(topic, pubkey, new_digest)
            .await
            .unwrap();

        reqwest::Client::new()
            .put(&url)
            .header("Content-Type", "application/octet-stream")
            .body(data.to_vec())
            .send()
            .await
            .unwrap();

        s3_store_vault
            .save_snapshot(topic, pubkey, prev_digest, new_digest)
            .await
            .unwrap();

        let get_url = s3_store_vault
            .get_snapshot_url(topic, pubkey)
            .await
            .unwrap()
            .unwrap();

        let response = reqwest::Client::new().get(&get_url).send().await.unwrap();
        assert_eq!(response.bytes().await.unwrap().as_ref(), data);

        // overwrite
        let data = b"test data 2";
        let prev_digest = Some(new_digest);
        let new_digest = get_digest(data);
        let url = s3_store_vault
            .pre_save_snapshot(topic, pubkey, new_digest)
            .await
            .unwrap();

        reqwest::Client::new()
            .put(&url)
            .header("Content-Type", "application/octet-stream")
            .body(data.to_vec())
            .send()
            .await
            .unwrap();

        s3_store_vault
            .save_snapshot(topic, pubkey, prev_digest, new_digest)
            .await
            .unwrap();

        let get_url = s3_store_vault
            .get_snapshot_url(topic, pubkey)
            .await
            .unwrap()
            .unwrap();
        let response = reqwest::Client::new().get(&get_url).send().await.unwrap();
        assert_eq!(response.bytes().await.unwrap().as_ref(), data);
    }

    /// test case 1: It is expected to get an error while preservation a snapshot for the absent previous digest.
    #[sqlx::test]
    async fn save_snapshot_with_invalid_prev_digest_test(pool: PgPool) {
        let _ = env_logger::builder().is_test(true).try_init();
        let vault = create_vault(
            pool,
            Config {
                s3_upload_timeout: 0,
                s3_download_timeout: 0,
                cleanup_interval: 0,
            },
        );

        let result = vault
            .save_snapshot(
                "topic",
                U256::from(1),
                Some(get_digest(b"test data 1")),
                get_digest(b"test data 2"),
            )
            .await;

        // test case 1
        assert!(matches!(result, Err(StoreVaultError::LockError(_))))
    }

    /// test case 1: It is expected to remove the existing object in the S3 while preservation a new object with the same public key and topic.
    ///
    /// test case 2: it is expected to update the timestep and digest while preservation a new object with the same public key and topic.
    #[sqlx::test]
    async fn save_snapshot_with_existed_digest_test(pool: PgPool) {
        let _ = env_logger::builder().is_test(true).try_init();
        let config = Config {
            s3_upload_timeout: 10,
            s3_download_timeout: 0,
            cleanup_interval: 0,
        };
        let mut vault = create_vault(pool, config.clone());

        let topic = "topic";
        let pubkey = U256::from(1);
        let digest_1 = get_digest(b"test data 1");
        let digest_2 = get_digest(b"test data 2");
        let path_1 = get_path(topic, pubkey, digest_1);
        let path_2 = get_path(topic, pubkey, digest_2);

        // Set up expectations for the first pre_save_snapshot call
        vault
            .s3_client
            .expect_generate_upload_url()
            .with(
                eq(path_1.clone()),
                eq("application/octet-stream"),
                eq(Duration::from_secs(config.s3_upload_timeout)),
            )
            .returning(|_, _, _| Ok("presigned_url_1".to_string()));

        // First, set up pre_save_snapshot and save_snapshot for digest_1
        vault
            .pre_save_snapshot(topic, pubkey, digest_1)
            .await
            .unwrap();

        // Set up check_object_exists expectation for the first save_snapshot call
        vault
            .s3_client
            .expect_check_object_exists()
            .with(eq(path_1.clone()))
            .returning(|_| Ok(true));

        vault
            .save_snapshot(topic, pubkey, None, digest_1)
            .await
            .unwrap();
        let (_, timestamp_stage_1) = select_snapshot(pubkey, topic, &vault.pool).await;

        // Set up expectations for the second pre_save_snapshot call
        vault
            .s3_client
            .expect_generate_upload_url()
            .with(
                eq(path_2.clone()),
                eq("application/octet-stream"),
                eq(Duration::from_secs(config.s3_upload_timeout)),
            )
            .returning(|_, _, _| Ok("presigned_url_2".to_string()));

        // Set up pre_save_snapshot for digest_2
        vault
            .pre_save_snapshot(topic, pubkey, digest_2)
            .await
            .unwrap();

        // Set up check_object_exists expectation for the second save_snapshot call
        vault
            .s3_client
            .expect_check_object_exists()
            .with(eq(path_2.clone()))
            .returning(|_| Ok(true));

        // Set up delete_object expectation for the second save_snapshot call
        vault
            .s3_client
            .expect_delete_object()
            .with(eq(path_1.clone()))
            .returning(|_| Ok(()));

        vault
            .save_snapshot(topic, pubkey, Some(digest_1), digest_2)
            .await
            .unwrap();
        let (digest_stage_2, timestamp_stage_2) = select_snapshot(pubkey, topic, &vault.pool).await;

        // test case 1
        assert_eq!(Bytes32::from_hex(&digest_stage_2).unwrap(), digest_2);
        // test case 2
        assert!(timestamp_stage_2 >= timestamp_stage_1);
    }

    /// test case 1: It is expected to receive None if there is no data on the given topic and public key in the database.
    ///
    /// test case 2: It is expected to get the correct URL if the database has data on a given topic and public key.
    #[sqlx::test]
    async fn get_snapshot_url_test(pool: PgPool) {
        let _ = env_logger::builder().is_test(true).try_init();
        let mut vault = create_vault(
            pool,
            Config {
                s3_upload_timeout: 0,
                s3_download_timeout: 0,
                cleanup_interval: 0,
            },
        );

        let topic = "topic";
        let pubkey = U256::from(1);
        let digest = get_digest(b"test data");
        let digest_path = get_path(topic, pubkey, digest);

        let url = vault.get_snapshot_url(topic, pubkey).await.unwrap();
        // test case 1
        assert_eq!(url, None);

        insert_snapshot(topic, pubkey, digest, &vault.pool).await;
        // Returns the URL equal to the transferred path
        vault
            .s3_client
            .expect_generate_download_url()
            .returning(|path, _| Ok(path.to_owned()));

        let url = vault.get_snapshot_url(topic, pubkey).await.unwrap();
        // test case 2
        assert_eq!(url, Some(digest_path));
    }

    /// test case 1: It is expected to get a correct URLS list while preservation a butch data into history.
    ///
    /// test case 2: It is expected to get an error while preservation an existing digest in history.
    ///
    /// test case 2: It is expected that the upload_finished flag will not be set for just saved data.
    #[sqlx::test]
    async fn batch_save_data_url_test(pool: PgPool) {
        let _ = env_logger::builder().is_test(true).try_init();
        let mut vault = create_vault(
            pool,
            Config {
                s3_upload_timeout: 0,
                s3_download_timeout: 0,
                cleanup_interval: 0,
            },
        );

        let entry_1 = S3SaveDataEntry {
            topic: "topic_1".to_owned(),
            pubkey: U256::from(1),
            digest: get_digest(b"test data 1"),
        };
        let entry_1_digest_path = get_path(&entry_1.topic, entry_1.pubkey, entry_1.digest);

        // Returns the URL equal to the transferred path
        vault
            .s3_client
            .expect_generate_upload_url()
            .returning(|path, _, _| Ok(path.to_owned()));

        let urls = vault.batch_save_data_url(&[entry_1.clone()]).await.unwrap();
        // test case 1
        assert_eq!(urls, vec![entry_1_digest_path]);

        let result = vault.batch_save_data_url(&[entry_1]).await;
        // test case 2
        assert!(matches!(result, Err(StoreVaultError::SaveHistoryError(_))));

        let data = select_s3_historical_data(&vault.pool).await;
        // test case 3
        assert!(data.iter().all(|(_, uf)| !uf));
    }

    /// test case 1: It is expected to get an empty urls list when requesting a history with existing topic and pubkey, but absent digests.
    ///
    /// test case 2: It is expected to get an empty urls list when requesting a history with existing digests, but absent topic and pubkey.
    ///
    /// test case 3: It is expected to get a correct urls list when requesting a history with existing topic, pubkey and digests.
    #[sqlx::test]
    async fn get_data_batch_test(pool: PgPool) {
        let _ = env_logger::builder().is_test(true).try_init();
        let mut vault = create_vault(
            pool,
            Config {
                s3_upload_timeout: 0,
                s3_download_timeout: 0,
                cleanup_interval: 0,
            },
        );

        let topic = "topic";
        let pubkey = U256::from(1);
        let digest = get_digest(b"test data");
        let digest_path = get_path(topic, pubkey, digest);

        // save test data
        vault
            .s3_client
            .expect_generate_upload_url()
            .returning(|_, _, _| Ok(String::new()));
        vault
            .batch_save_data_url(&[S3SaveDataEntry {
                topic: topic.to_owned(),
                pubkey,
                digest,
            }])
            .await
            .unwrap();

        // Returns the URL equal to the transferred path
        vault
            .s3_client
            .expect_generate_download_url()
            .returning(|path, _| Ok(path.to_owned()));

        let urls = vault
            .get_data_batch(topic, pubkey, &[get_digest(b"non existent data")])
            .await
            .unwrap();
        // test case 1
        assert!(urls.is_empty());

        let urls = vault
            .get_data_batch("non existent topic", U256::from(u32::MAX), &[digest])
            .await
            .unwrap();
        // test case 2
        assert!(urls.is_empty());

        let urls = vault
            .get_data_batch(topic, pubkey, &[digest])
            .await
            .unwrap();
        // test case 3
        assert_eq!(
            urls.iter()
                .map(|u| u.presigned_url.clone())
                .collect::<Vec<_>>(),
            vec![digest_path]
        );
        assert_eq!(
            urls.iter().map(|u| u.meta.digest).collect::<Vec<_>>(),
            vec![digest]
        );
    }

    /// test case 1: It is expected to get an empty urls and metadatas list when requesting a history with absent topic and pubkey.
    ///
    /// test case 2: Correct behavior of data requests sorted by increasing is expected:
    /// at the first request, the metadata contains a cursor that can be used in the second request,
    /// the second request that receives all the remaining data (exclude cursor).
    ///
    /// test case 3: Similarly, test case 2, but the sorting of decrease is used.
    #[sqlx::test]
    async fn get_data_sequence_url_test(pool: PgPool) {
        let _ = env_logger::builder().is_test(true).try_init();
        let mut vault = create_vault(
            pool,
            Config {
                s3_upload_timeout: 0,
                s3_download_timeout: 0,
                cleanup_interval: 0,
            },
        );

        let topic = "topic";
        let pubkey = U256::from(1);
        let entry_1 = S3SaveDataEntry {
            topic: topic.to_owned(),
            pubkey,
            digest: get_digest(b"test data 1"),
        };
        let entry_2 = S3SaveDataEntry {
            topic: topic.to_owned(),
            pubkey,
            digest: get_digest(b"test data 2"),
        };
        let entry_3 = S3SaveDataEntry {
            topic: topic.to_owned(),
            pubkey,
            digest: get_digest(b"test data 3"),
        };
        let entry_1_digest_path = get_path(&entry_1.topic, entry_1.pubkey, entry_1.digest);
        let entry_2_digest_path = get_path(&entry_2.topic, entry_2.pubkey, entry_2.digest);
        let entry_3_digest_path = get_path(&entry_3.topic, entry_3.pubkey, entry_3.digest);

        // save test data
        vault
            .s3_client
            .expect_generate_upload_url()
            .returning(|_, _, _| Ok(String::new()));
        vault.batch_save_data_url(&[entry_1.clone()]).await.unwrap();
        sleep(Duration::from_secs(1)).await;
        vault.batch_save_data_url(&[entry_2.clone()]).await.unwrap();
        sleep(Duration::from_secs(1)).await;
        vault.batch_save_data_url(&[entry_3.clone()]).await.unwrap();

        // Returns the URL equal to the transferred path
        vault
            .s3_client
            .expect_generate_download_url()
            .returning(|path, _| Ok(path.to_owned()));

        let (urls, metadata) = vault
            .get_data_sequence_url(
                "non existent topic",
                U256::from(u32::MAX),
                &MetaDataCursor::default(),
            )
            .await
            .unwrap();
        // test case 1
        assert!(urls.is_empty());
        assert!(metadata.next_cursor.is_none());
        assert!(!metadata.has_more);
        assert_eq!(metadata.total_count, 3);

        let (urls, metadata) = vault
            .get_data_sequence_url(
                topic,
                pubkey,
                &MetaDataCursor {
                    cursor: None,
                    order: CursorOrder::Asc,
                    limit: Some(2),
                },
            )
            .await
            .unwrap();
        let next_cursor = metadata.next_cursor.unwrap();
        // test case 2
        assert_eq!(
            urls.iter()
                .map(|u| u.presigned_url.clone())
                .collect::<Vec<_>>(),
            vec![entry_1_digest_path.clone(), entry_2_digest_path.clone()]
        );
        assert_eq!(
            urls.iter().map(|u| u.meta.digest).collect::<Vec<_>>(),
            vec![entry_1.digest, entry_2.digest]
        );
        assert_eq!(next_cursor.digest, entry_2.digest);
        assert!(metadata.has_more);
        assert_eq!(metadata.total_count, 3);

        let (urls, metadata) = vault
            .get_data_sequence_url(
                topic,
                pubkey,
                &MetaDataCursor {
                    cursor: Some(next_cursor),
                    order: CursorOrder::Asc,
                    limit: None,
                },
            )
            .await
            .unwrap();
        let next_cursor = metadata.next_cursor.unwrap();
        // test case 2
        assert_eq!(
            urls.iter()
                .map(|u| u.presigned_url.clone())
                .collect::<Vec<_>>(),
            vec![entry_3_digest_path.clone()]
        );
        assert_eq!(
            urls.iter().map(|u| u.meta.digest).collect::<Vec<_>>(),
            vec![entry_3.digest]
        );
        assert_eq!(next_cursor.digest, entry_3.digest);
        assert!(!metadata.has_more);
        assert_eq!(metadata.total_count, 3);

        let (urls, metadata) = vault
            .get_data_sequence_url(
                topic,
                pubkey,
                &MetaDataCursor {
                    cursor: None,
                    order: CursorOrder::Desc,
                    limit: Some(2),
                },
            )
            .await
            .unwrap();
        let next_cursor = metadata.next_cursor.unwrap();
        // test case 3
        assert_eq!(
            urls.iter()
                .map(|u| u.presigned_url.clone())
                .collect::<Vec<_>>(),
            vec![entry_3_digest_path, entry_2_digest_path]
        );
        assert_eq!(
            urls.iter().map(|u| u.meta.digest).collect::<Vec<_>>(),
            vec![entry_3.digest, entry_2.digest]
        );
        assert_eq!(next_cursor.digest, entry_2.digest);
        assert!(metadata.has_more);
        assert_eq!(metadata.total_count, 3);

        let (urls, metadata) = vault
            .get_data_sequence_url(
                topic,
                pubkey,
                &MetaDataCursor {
                    cursor: Some(next_cursor),
                    order: CursorOrder::Desc,
                    limit: None,
                },
            )
            .await
            .unwrap();
        let next_cursor = metadata.next_cursor.unwrap();
        // test case 3
        assert_eq!(
            urls.iter()
                .map(|u| u.presigned_url.clone())
                .collect::<Vec<_>>(),
            vec![entry_1_digest_path]
        );
        assert_eq!(
            urls.iter().map(|u| u.meta.digest).collect::<Vec<_>>(),
            vec![entry_1.digest]
        );
        assert_eq!(next_cursor.digest, entry_1.digest);
        assert!(!metadata.has_more);
        assert_eq!(metadata.total_count, 3);
    }

    /// test case 1: It is expected that the upload_finished flag will be installed only for the data that is exist in S3.
    ///
    /// test case 2: It is expected that the data that are not found in the S3 will not be affected if upload timeout has not yet expired.
    ///
    /// test case 3: It is expected that the data that are not found in the S3 will be deleted if the upload timeout is expired.
    #[sqlx::test]
    async fn cleanup_data_test(pool: PgPool) {
        let _ = env_logger::builder().is_test(true).try_init();
        const S3_UPLOAD_TIMEOUT: u64 = 3;
        let mut vault = create_vault(
            pool,
            Config {
                s3_upload_timeout: S3_UPLOAD_TIMEOUT,
                s3_download_timeout: 0,
                cleanup_interval: 0,
            },
        );

        let topic = "topic";
        let pubkey = U256::from(1);
        let entry_1 = S3SaveDataEntry {
            topic: topic.to_owned(),
            pubkey,
            digest: get_digest(b"test data 1"),
        };
        let entry_2 = S3SaveDataEntry {
            topic: topic.to_owned(),
            pubkey,
            digest: get_digest(b"test data 2"),
        };
        let entry_3 = S3SaveDataEntry {
            topic: topic.to_owned(),
            pubkey,
            digest: get_digest(b"test data 3"),
        };
        let entry_1_digest_path = get_path(&entry_1.topic, entry_1.pubkey, entry_1.digest);
        let entry_2_digest_path = get_path(&entry_2.topic, entry_2.pubkey, entry_2.digest);
        let entry_3_digest_path = get_path(&entry_3.topic, entry_3.pubkey, entry_3.digest);

        // save test data
        vault
            .s3_client
            .expect_generate_upload_url()
            .returning(|_, _, _| Ok(String::new()));
        vault.batch_save_data_url(&[entry_1.clone()]).await.unwrap();
        vault.batch_save_data_url(&[entry_2.clone()]).await.unwrap();
        sleep(Duration::from_secs(S3_UPLOAD_TIMEOUT)).await;
        vault.batch_save_data_url(&[entry_3.clone()]).await.unwrap();

        // Returns the existence of an object for each path
        {
            let entry_1_digest_path = entry_1_digest_path.clone();
            let entry_2_digest_path = entry_2_digest_path.clone();
            let entry_3_digest_path = entry_3_digest_path.clone();
            vault
                .s3_client
                .expect_check_object_exists()
                .returning(move |path| {
                    let is_exist = match path {
                        p if p == entry_1_digest_path => true,
                        p if p == entry_2_digest_path => false,
                        p if p == entry_3_digest_path => false,
                        _ => panic!("testing data initialization error"),
                    };
                    Ok(is_exist)
                });
        }

        vault.cleanup_historical_data().await.unwrap();
        let remaining_data = select_s3_historical_data(&vault.pool).await;
        // test case 1
        assert!(remaining_data
            .iter()
            .filter(|(p, _)| *p == entry_1_digest_path)
            .all(|(_, uf)| *uf));
        // test case 2
        assert!(remaining_data
            .iter()
            .filter(|(p, _)| *p == entry_3_digest_path)
            .all(|(_, uf)| !*uf));
        // test case 3
        assert_eq!(
            remaining_data
                .iter()
                .filter(|(p, _)| *p == entry_2_digest_path)
                .count(),
            0
        );
    }

    fn create_vault(pool: PgPool, config: Config) -> S3StoreVault {
        let pool = DbPool::new(pool);
        let s3_client = S3Client::default();

        S3StoreVault {
            config,
            pool,
            s3_client,
        }
    }

    async fn select_snapshot(
        pubkey: U256,
        topic: &str,
        executor: impl Executor<'_, Database = Postgres>,
    ) -> (String, i64) {
        let record = sqlx::query!(
            r#"
            SELECT digest, timestamp FROM s3_snapshot_data
            WHERE pubkey = $1 AND topic = $2
            "#,
            pubkey.to_hex(),
            topic,
        )
        .fetch_one(executor)
        .await
        .unwrap();

        (record.digest, record.timestamp)
    }

    async fn insert_snapshot(
        topic: &str,
        pubkey: U256,
        digest: Bytes32,
        executor: impl Executor<'_, Database = Postgres>,
    ) {
        sqlx::query!(
            r#"
            INSERT INTO s3_snapshot_data (pubkey, topic, digest, timestamp)
            VALUES ($1, $2, $3, $4)
            "#,
            pubkey.to_hex(),
            topic,
            digest.to_hex(),
            chrono::Utc::now().timestamp() as i64
        )
        .execute(executor)
        .await
        .unwrap();
    }

    async fn select_s3_historical_data(
        executor: impl Executor<'_, Database = Postgres>,
    ) -> Vec<(String, bool)> {
        let records = sqlx::query!(
            r#"
            SELECT digest, upload_finished
            FROM s3_historical_data
            "#,
        )
        .fetch_all(executor)
        .await
        .unwrap();

        records
            .into_iter()
            .map(|r| (r.digest, r.upload_finished))
            .collect()
    }
}
