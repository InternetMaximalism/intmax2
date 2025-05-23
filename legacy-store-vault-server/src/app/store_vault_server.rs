use intmax2_interfaces::{
    api::store_vault_server::{
        interface::{SaveDataEntry, MAX_BATCH_SIZE},
        types::{CursorOrder, DataWithMetaData, MetaDataCursor, MetaDataCursorResponse},
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
                        "prev_digest {prev_digest} mismatch with stored digest {digest}"
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

        let result = sqlx::query!(
            r#"
            INSERT INTO historical_data (digest, pubkey, topic, data, timestamp)
            SELECT
                UNNEST($1::text[]),
                UNNEST($2::text[]),
                UNNEST($3::text[]),
                UNNEST($4::bytea[]),
                UNNEST($5::bigint[])
            "#,
            &digests_hex,
            &pubkeys,
            &topics,
            &data,
            &timestamps
        )
        .execute(&self.pool)
        .await;

        match result {
            Ok(_) => {}
            Err(sqlx::Error::Database(error))
                if error.constraint() == Some("historical_data_pkey") =>
            {
                return Err(StoreVaultError::SaveHistoryError(
                    "data with the specified digest already in history".to_owned(),
                ))
            }
            Err(err) => return Err(err.into()),
        }

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
            SELECT data, timestamp, digest
            FROM historical_data
            WHERE topic = $1 AND pubkey = $2 AND digest = ANY($3)
            "#,
            topic,
            pubkey_hex,
            &digests_hex
        )
        .fetch_all(&self.pool)
        .await?;

        let result: Vec<DataWithMetaData> = records
            .into_iter()
            .map(|r| DataWithMetaData {
                data: r.data,
                meta: MetaData {
                    digest: Bytes32::from_hex(&r.digest).unwrap(),
                    timestamp: r.timestamp as u64,
                },
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

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use intmax2_interfaces::utils::digest::get_digest;
    use intmax2_zkp::ethereum_types::u256::U256;
    use sqlx::{Executor, PgPool, Postgres};
    use tokio::time::sleep;

    use super::*;

    /// test case 1: It is expected to get an error while preservation a snapshot for the not existing previous digest.
    ///
    /// test case 2: It is expected to get an error while preservation a snapshot for the invalid previous digest.
    ///
    /// test case 3: It is expected to get an error while preservation a snapshot for the None previous digest but previous object is exist.
    #[sqlx::test]
    async fn save_snapshot_with_invalid_digest_test(pool: PgPool) {
        let _ = env_logger::builder().is_test(true).try_init();
        let vault = StoreVaultServer {
            pool: DbPool::new(pool),
        };

        let topic = "topic";
        let pubkey = U256::from(1);
        let data_1 = b"test data 1";
        let data_2 = b"test data 2";
        let digest_1 = get_digest(data_1);
        let digest_2 = get_digest(data_2);

        let result = vault
            .save_snapshot(topic, pubkey, Some(digest_1), data_2)
            .await;
        // test case 1
        assert!(matches!(result, Err(StoreVaultError::LockError(_))));

        vault
            .save_snapshot(topic, pubkey, None, data_1)
            .await
            .unwrap();

        let result = vault
            .save_snapshot(topic, pubkey, Some(digest_2), data_2)
            .await;
        // test case 2
        assert!(matches!(result, Err(StoreVaultError::LockError(_))));

        let result = vault.save_snapshot(topic, pubkey, None, data_2).await;
        // test case 3
        assert!(matches!(result, Err(StoreVaultError::LockError(_))));
    }

    /// test case 1: it is expected to update the timestep, digest and data while preservation a new object with the same public key and topic.
    #[sqlx::test]
    async fn save_snapshot_with_existed_digest_test(pool: PgPool) {
        let _ = env_logger::builder().is_test(true).try_init();
        let vault = StoreVaultServer {
            pool: DbPool::new(pool),
        };

        let topic = "topic";
        let pubkey = U256::from(1);
        let data_1 = b"test data 1";
        let data_2 = b"test data 2";
        let digest_1 = get_digest(data_1);
        let digest_2 = get_digest(data_2);

        vault
            .save_snapshot(topic, pubkey, None, data_1)
            .await
            .unwrap();
        let (_, _, timestamp_stage_1) = select_snapshot(pubkey, topic, &vault.pool).await;

        vault
            .save_snapshot(topic, pubkey, Some(digest_1), data_2)
            .await
            .unwrap();
        let (data_stage_2, digest_stage_2, timestamp_stage_2) =
            select_snapshot(pubkey, topic, &vault.pool).await;

        // test case 1
        assert!(timestamp_stage_2 >= timestamp_stage_1);
        assert_eq!(Bytes32::from_hex(&digest_stage_2).unwrap(), digest_2);
        assert_eq!(data_stage_2, data_2);
    }

    /// test case 1: It is expected to get a correct URLS list while preservation a butch data into history.
    ///
    /// test case 2: It is expected to get an error while preservation an existing digest in history.
    #[sqlx::test]
    async fn batch_save_data_test(pool: PgPool) {
        let _ = env_logger::builder().is_test(true).try_init();
        let vault = StoreVaultServer {
            pool: DbPool::new(pool),
        };

        let entry_1 = SaveDataEntry {
            topic: "topic".to_owned(),
            pubkey: U256::from(1),
            data: b"test data".to_vec(),
        };
        let entry_1_digest = get_digest(&entry_1.data);

        let urls = vault
            .batch_save_data(std::slice::from_ref(&entry_1))
            .await
            .unwrap();
        // test case 1
        assert_eq!(urls, vec![entry_1_digest]);

        let result = vault.batch_save_data(&[entry_1]).await;
        // test case 2
        assert!(matches!(result, Err(StoreVaultError::SaveHistoryError(_))));
    }

    /// test case 1: It is expected to get an empty list when requesting a history with existing topic and pubkey, but absent digests.
    ///
    /// test case 2: It is expected to get an empty list when requesting a history with existing digests, but absent topic and pubkey.
    ///
    /// test case 3: It is expected to get a correct list when requesting a history with existing topic, pubkey and digests.
    #[sqlx::test]
    async fn get_data_batch_test(pool: PgPool) {
        let _ = env_logger::builder().is_test(true).try_init();
        let vault = StoreVaultServer {
            pool: DbPool::new(pool),
        };

        let topic = "topic";
        let pubkey = U256::from(1);
        let data = b"test data";
        let digest = get_digest(data);

        // save test data
        vault
            .batch_save_data(&[SaveDataEntry {
                topic: topic.to_owned(),
                pubkey,
                data: data.to_vec(),
            }])
            .await
            .unwrap();

        let list = vault
            .get_data_batch(topic, pubkey, &[get_digest(b"non existent data")])
            .await
            .unwrap();
        // test case 1
        assert!(list.is_empty());

        let list = vault
            .get_data_batch("non existent topic", U256::from(u32::MAX), &[digest])
            .await
            .unwrap();
        // test case 2
        assert!(list.is_empty());

        let list = vault
            .get_data_batch(topic, pubkey, &[digest])
            .await
            .unwrap();
        // test case 3
        assert_eq!(
            list.iter().map(|u| u.data.clone()).collect::<Vec<_>>(),
            vec![data]
        );
        assert_eq!(
            list.iter().map(|u| u.meta.digest).collect::<Vec<_>>(),
            vec![digest]
        );
    }

    /// test case 1: It is expected to get an empty data and metadata list when requesting a history with absent topic and pubkey.
    ///
    /// test case 2: Correct behavior of data requests sorted by increasing is expected:
    /// at the first request, the metadata contains a cursor that can be used in the second request,
    /// the second request that receives all the remaining data (exclude cursor).
    ///
    /// test case 3: Similarly, test case 2, but the sorting of decrease is used.
    #[sqlx::test]
    async fn get_data_sequence_test(pool: PgPool) {
        let _ = env_logger::builder().is_test(true).try_init();
        let vault = StoreVaultServer {
            pool: DbPool::new(pool),
        };

        let topic = "topic";
        let pubkey = U256::from(1);
        let entry_1 = SaveDataEntry {
            topic: topic.to_owned(),
            pubkey,
            data: b"test data 1".to_vec(),
        };
        let entry_2 = SaveDataEntry {
            topic: topic.to_owned(),
            pubkey,
            data: b"test data 2".to_vec(),
        };
        let entry_3 = SaveDataEntry {
            topic: topic.to_owned(),
            pubkey,
            data: b"test data 3".to_vec(),
        };
        let entry_1_digest = get_digest(&entry_1.data);
        let entry_2_digest = get_digest(&entry_2.data);
        let entry_3_digest = get_digest(&entry_3.data);

        // save test data
        vault
            .batch_save_data(std::slice::from_ref(&entry_1))
            .await
            .unwrap();
        sleep(Duration::from_secs(1)).await;
        vault
            .batch_save_data(std::slice::from_ref(&entry_2))
            .await
            .unwrap();
        sleep(Duration::from_secs(1)).await;
        vault
            .batch_save_data(std::slice::from_ref(&entry_3))
            .await
            .unwrap();

        let (urls, metadata) = vault
            .get_data_sequence(
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
            .get_data_sequence(
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
            urls.iter().map(|u| u.data.clone()).collect::<Vec<_>>(),
            vec![entry_1.data.clone(), entry_2.data.clone()]
        );
        assert_eq!(
            urls.iter().map(|u| u.meta.digest).collect::<Vec<_>>(),
            vec![entry_1_digest, entry_2_digest]
        );
        assert_eq!(next_cursor.digest, entry_2_digest);
        assert!(metadata.has_more);
        assert_eq!(metadata.total_count, 3);

        let (urls, metadata) = vault
            .get_data_sequence(
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
            urls.iter().map(|u| u.data.clone()).collect::<Vec<_>>(),
            vec![entry_3.data.clone()]
        );
        assert_eq!(
            urls.iter().map(|u| u.meta.digest).collect::<Vec<_>>(),
            vec![entry_3_digest]
        );
        assert_eq!(next_cursor.digest, entry_3_digest);
        assert!(!metadata.has_more);
        assert_eq!(metadata.total_count, 3);

        let (urls, metadata) = vault
            .get_data_sequence(
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
            urls.iter().map(|u| u.data.clone()).collect::<Vec<_>>(),
            vec![entry_3.data, entry_2.data]
        );
        assert_eq!(
            urls.iter().map(|u| u.meta.digest).collect::<Vec<_>>(),
            vec![entry_3_digest, entry_2_digest]
        );
        assert_eq!(next_cursor.digest, entry_2_digest);
        assert!(metadata.has_more);
        assert_eq!(metadata.total_count, 3);

        let (urls, metadata) = vault
            .get_data_sequence(
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
            urls.iter().map(|u| u.data.clone()).collect::<Vec<_>>(),
            vec![entry_1.data]
        );
        assert_eq!(
            urls.iter().map(|u| u.meta.digest).collect::<Vec<_>>(),
            vec![entry_1_digest]
        );
        assert_eq!(next_cursor.digest, entry_1_digest);
        assert!(!metadata.has_more);
        assert_eq!(metadata.total_count, 3);
    }

    async fn select_snapshot(
        pubkey: U256,
        topic: &str,
        executor: impl Executor<'_, Database = Postgres>,
    ) -> (Vec<u8>, String, i64) {
        let record = sqlx::query!(
            r#"
            SELECT data, digest, timestamp FROM snapshot_data
            WHERE pubkey = $1 AND topic = $2
            "#,
            pubkey.to_hex(),
            topic,
        )
        .fetch_one(executor)
        .await
        .unwrap();

        (record.data, record.digest, record.timestamp)
    }
}
