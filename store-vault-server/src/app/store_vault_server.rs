use anyhow::{anyhow, Ok, Result};
use intmax2_interfaces::{
    api::store_vault_server::{
        interface::{DataType, SaveDataEntry, MAX_BATCH_SIZE},
        types::{CursorOrder, DataWithMetaData, MetaDataCursor, MetaDataCursorResponse},
    },
    data::meta_data::MetaData,
    utils::digest::get_digest,
};
use intmax2_zkp::ethereum_types::{bytes32::Bytes32, u256::U256, u32limb_trait::U32LimbTrait};

use sqlx::Postgres;
use uuid::Uuid;

use server_common::db::{DbPool, DbPoolConfig};

use crate::{app::utils::extract_timestamp_from_uuidv7, EnvVar};

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

    pub async fn save_user_data(
        &self,
        pubkey: U256,
        prev_digest: Option<Bytes32>,
        encrypted_data: &[u8],
    ) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        let result = self.get_user_data_and_digest(&mut tx, pubkey).await?;
        // validation
        if let Some(prev_digest) = prev_digest {
            if let Some((_, digest)) = result {
                if digest != prev_digest {
                    return Err(anyhow!(
                        "Prev digest mismatch {} != {}",
                        digest,
                        prev_digest
                    ));
                }
            } else {
                return Err(anyhow!(
                    "User data not found though prev_digest is provided"
                ));
            }
        } else if result.is_some() {
            return Err(anyhow!(
                "User data already exists but prev_digest is not provided"
            ));
        }
        let pubkey_hex = pubkey.to_hex();
        let digest = get_digest(encrypted_data);
        let digest_serialized = digest.to_bytes_be();
        sqlx::query!(
            r#"
            INSERT INTO encrypted_user_data (pubkey, encrypted_data, digest, timestamp)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (pubkey) DO UPDATE SET encrypted_data = EXCLUDED.encrypted_data,
            digest = EXCLUDED.digest, timestamp = EXCLUDED.timestamp
            "#,
            pubkey_hex,
            encrypted_data,
            digest_serialized,
            chrono::Utc::now().timestamp() as i64
        )
        .execute(tx.as_mut())
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn get_user_data(&self, pubkey: U256) -> Result<Option<Vec<u8>>> {
        let mut tx = self.pool.begin().await?;
        let result = self.get_user_data_and_digest(&mut tx, pubkey).await?;
        tx.commit().await?;
        Ok(result.map(|(data, _)| data))
    }

    async fn get_user_data_and_digest(
        &self,
        tx: &mut sqlx::Transaction<'_, Postgres>,
        pubkey: U256,
    ) -> Result<Option<(Vec<u8>, Bytes32)>> {
        let pubkey_hex = pubkey.to_hex();
        let record = sqlx::query!(
            r#"
            SELECT encrypted_data, digest FROM encrypted_user_data WHERE pubkey = $1
            "#,
            pubkey_hex
        )
        .fetch_optional(tx.as_mut())
        .await?;
        Ok(record.map(|r| (r.encrypted_data, Bytes32::from_bytes_be(&r.digest))))
    }

    pub async fn save_sender_proof_set(
        &self,
        ephemeral_pubkey: U256,
        encrypted_data: &[u8],
    ) -> Result<()> {
        let pubkey_hex = ephemeral_pubkey.to_hex();
        sqlx::query!(
            r#"
            INSERT INTO encrypted_sender_proof_set (pubkey, encrypted_data)
            VALUES ($1, $2)
            ON CONFLICT (pubkey) DO NOTHING
            "#,
            pubkey_hex,
            encrypted_data
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_sender_proof_set(&self, ephemeral_pubkey: U256) -> Result<Vec<u8>> {
        let pubkey_hex = ephemeral_pubkey.to_hex();
        let record = sqlx::query!(
            r#"
            SELECT encrypted_data FROM encrypted_sender_proof_set WHERE pubkey = $1
            "#,
            pubkey_hex
        )
        .fetch_optional(&self.pool)
        .await?;
        if record.is_none() {
            return Err(anyhow!("Sender proof set not found"));
        }
        Ok(record.unwrap().encrypted_data)
    }

    pub async fn batch_save_data(&self, entries: &[SaveDataEntry]) -> Result<Vec<String>> {
        // Prepare values for bulk insert
        let data_types: Vec<i32> = entries.iter().map(|entry| entry.data_type as i32).collect();
        let pubkeys: Vec<String> = entries.iter().map(|entry| entry.pubkey.to_hex()).collect();
        let uuids: Vec<String> = (0..entries.len())
            .map(|_| Uuid::now_v7().to_string())
            .collect();
        let encrypted_data: Vec<Vec<u8>> = entries
            .iter()
            .map(|entry| entry.encrypted_data.clone())
            .collect();

        // Execute the bulk insert
        sqlx::query!(
            r#"
            INSERT INTO encrypted_data 
            (data_type, pubkey, uuid, encrypted_data)
            SELECT 
                UNNEST($1::integer[]),
                UNNEST($2::text[]),
                UNNEST($3::text[]),
                UNNEST($4::bytea[])
            "#,
            &data_types,
            &pubkeys,
            &uuids,
            &encrypted_data,
        )
        .execute(&self.pool)
        .await?;

        Ok(uuids)
    }

    pub async fn get_data_batch(
        &self,
        data_type: DataType,
        pubkey: U256,
        uuids: &[String],
    ) -> Result<Vec<DataWithMetaData>> {
        let pubkey_hex = pubkey.to_hex();
        let records = sqlx::query!(
            r#"
            SELECT uuid, encrypted_data
            FROM encrypted_data
            WHERE data_type = $1 AND pubkey = $2 AND uuid = ANY($3)
            "#,
            data_type as i32,
            pubkey_hex,
            &uuids
        )
        .fetch_all(&self.pool)
        .await?;
        let result: Vec<DataWithMetaData> = records
            .into_iter()
            .map(|r| {
                let u = Uuid::parse_str(r.uuid.as_str()).unwrap_or_default();
                let meta = MetaData {
                    uuid: r.uuid,
                    timestamp: extract_timestamp_from_uuidv7(&u).0,
                };
                DataWithMetaData {
                    meta,
                    data: r.encrypted_data,
                }
            })
            .collect();
        Ok(result)
    }

    pub async fn get_data_sequence(
        &self,
        data_type: DataType,
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
            SELECT uuid, encrypted_data
            FROM encrypted_data
            WHERE data_type = $1 
            AND pubkey = $2 
            AND uuid > $3
            ORDER BY uuid ASC
            LIMIT $4
        "#,
                    data_type as i32,
                    pubkey_hex,
                    cursor_meta.uuid,
                    actual_limit + 1
                )
                .fetch_all(&self.pool)
                .await?
                .into_iter()
                .map(|r| {
                    let u = Uuid::parse_str(r.uuid.as_str()).unwrap_or_default();
                    let meta = MetaData {
                        uuid: r.uuid,
                        timestamp: extract_timestamp_from_uuidv7(&u).0,
                    };
                    DataWithMetaData {
                        meta,
                        data: r.encrypted_data,
                    }
                })
                .collect()
            }
            CursorOrder::Desc => {
                let uuid = cursor
                    .cursor
                    .as_ref()
                    .map(|c| c.uuid.clone())
                    .unwrap_or(Uuid::max().to_string());
                sqlx::query!(
                    r#"
            SELECT uuid, encrypted_data
            FROM encrypted_data
            WHERE data_type = $1 
            AND pubkey = $2 
            AND uuid < $3
            ORDER BY uuid DESC
            LIMIT $4
        "#,
                    data_type as i32,
                    pubkey_hex,
                    uuid,
                    actual_limit + 1
                )
                .fetch_all(&self.pool)
                .await?
                .into_iter()
                .map(|r| {
                    let u = Uuid::parse_str(r.uuid.as_str()).unwrap_or_default();
                    let meta = MetaData {
                        uuid: r.uuid,
                        timestamp: extract_timestamp_from_uuidv7(&u).0,
                    };
                    DataWithMetaData {
                        meta,
                        data: r.encrypted_data,
                    }
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
            SELECT COUNT(*) FROM encrypted_data
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

    pub async fn save_misc(
        &self,
        pubkey: U256,
        topic: Bytes32,
        encrypted_data: &[u8],
    ) -> Result<String> {
        let pubkey_hex = pubkey.to_hex();
        let topic_hex = topic.to_hex();
        let uuid = Uuid::new_v4().to_string();
        sqlx::query!(
            r#"
            INSERT INTO encrypted_misc (uuid, topic, pubkey, encrypted_data, timestamp)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (uuid) DO NOTHING
            "#,
            uuid,
            topic_hex,
            pubkey_hex,
            encrypted_data,
            chrono::Utc::now().timestamp() as i64
        )
        .execute(&self.pool)
        .await?;
        Ok(uuid)
    }

    pub async fn get_misc_sequence(
        &self,
        pubkey: U256,
        topic: Bytes32,
        cursor: &MetaDataCursor,
    ) -> Result<(Vec<DataWithMetaData>, MetaDataCursorResponse)> {
        let pubkey_hex = pubkey.to_hex();
        let topic_hex = topic.to_hex();
        let actual_limit = cursor.limit.unwrap_or(MAX_BATCH_SIZE as u32) as i64;

        let result: Vec<DataWithMetaData> = match cursor.order {
            CursorOrder::Asc => {
                let cursor_meta = cursor.cursor.clone().unwrap_or_default();
                sqlx::query!(
                    r#"
            SELECT uuid, timestamp, encrypted_data
            FROM encrypted_misc
            WHERE topic = $1 
            AND pubkey = $2 
            AND (timestamp, uuid) > ($3, $4)
            ORDER BY timestamp ASC, uuid ASC
            LIMIT $5
        "#,
                    topic_hex,
                    pubkey_hex,
                    cursor_meta.timestamp as i64,
                    cursor_meta.uuid,
                    actual_limit + 1
                )
                .fetch_all(&self.pool)
                .await?
                .into_iter()
                .map(|r| {
                    let meta = MetaData {
                        uuid: r.uuid,
                        timestamp: r.timestamp as u64,
                    };
                    DataWithMetaData {
                        meta,
                        data: r.encrypted_data,
                    }
                })
                .collect()
            }
            CursorOrder::Desc => {
                let timestamp = cursor
                    .cursor
                    .as_ref()
                    .map(|c| c.timestamp as i64)
                    .unwrap_or_else(|| i64::MAX);
                let uuid = cursor
                    .cursor
                    .as_ref()
                    .map(|c| c.uuid.clone())
                    .unwrap_or_default();
                sqlx::query!(
                    r#"
            SELECT uuid, timestamp, encrypted_data
            FROM encrypted_misc
            WHERE topic = $1
            AND pubkey = $2
            AND (timestamp, uuid) < ($3, $4)
            ORDER BY timestamp DESC, uuid DESC
            LIMIT $5
        "#,
                    topic_hex,
                    pubkey_hex,
                    timestamp,
                    uuid,
                    actual_limit + 1
                )
                .fetch_all(&self.pool)
                .await?
                .into_iter()
                .map(|r| {
                    let meta = MetaData {
                        uuid: r.uuid,
                        timestamp: r.timestamp as u64,
                    };
                    DataWithMetaData {
                        meta,
                        data: r.encrypted_data,
                    }
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
            SELECT COUNT(*) FROM encrypted_misc
            WHERE topic = $1 AND pubkey = $2
            "#,
            topic_hex,
            pubkey_hex
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
    use ethers::core::rand;
    use intmax2_interfaces::{
        data::{encryption::BlsEncryption as _, meta_data::MetaData, user_data::UserData},
        utils::digest::get_digest,
    };
    use intmax2_zkp::common::signature::key_set::KeySet;

    use crate::{app::store_vault_server::StoreVaultServer, EnvVar};

    #[tokio::test]
    async fn test_user_data_get_and_save() -> anyhow::Result<()> {
        dotenv::dotenv().ok();
        let env: EnvVar = envy::from_env()?;
        let store_vault_server = StoreVaultServer::new(&env).await?;
        let mut rng = rand::thread_rng();
        let key = KeySet::rand(&mut rng);
        let encrypted_user_data = store_vault_server.get_user_data(key.pubkey).await?;
        assert!(encrypted_user_data.is_none());

        let mut user_data = UserData::new(key.pubkey);
        let encrypted = user_data.encrypt(key.pubkey);
        let digest = get_digest(&encrypted);
        store_vault_server
            .save_user_data(key.pubkey, None, &encrypted)
            .await?;
        let got_encrypted_user_data = store_vault_server.get_user_data(key.pubkey).await?;
        assert_eq!(got_encrypted_user_data.as_ref().unwrap(), &encrypted);
        let digest2 = get_digest(&got_encrypted_user_data.unwrap());
        assert_eq!(digest, digest2);
        user_data.deposit_status.last_processed_meta_data = Some(MetaData {
            timestamp: 1,
            uuid: "test".to_string(),
        });
        let encrypted = user_data.encrypt(key.pubkey);
        let digest3 = get_digest(&encrypted);
        store_vault_server
            .save_user_data(key.pubkey, Some(digest), &encrypted)
            .await?;

        user_data.deposit_status.last_processed_meta_data = Some(MetaData {
            timestamp: 2,
            uuid: "test2".to_string(),
        });
        store_vault_server
            .save_user_data(key.pubkey, Some(digest3), &encrypted)
            .await?;
        Ok(())
    }
}
