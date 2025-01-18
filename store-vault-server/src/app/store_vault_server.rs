use std::time::Duration;

use anyhow::{anyhow, Ok, Result};
use intmax2_interfaces::{
    api::store_vault_server::{
        interface::{DataType, SaveDataEntry},
        types::{DataWithMetaData, MetaDataCursor, MetaDataCursorResponse},
    },
    data::meta_data::MetaData,
    utils::digest::get_digest,
};
use intmax2_zkp::ethereum_types::{bytes32::Bytes32, u256::U256, u32limb_trait::U32LimbTrait};

use sqlx::{postgres::PgPoolOptions, PgPool, Postgres};
use uuid::Uuid;

use crate::EnvVar;

pub struct Config {
    pub max_pagination_limit: u32,
}

pub struct StoreVaultServer {
    pool: PgPool,
    config: Config,
}

impl StoreVaultServer {
    pub async fn new(env: &EnvVar) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(env.database_max_connections)
            .idle_timeout(Duration::from_secs(env.database_timeout))
            .connect(&env.database_url)
            .await?;
        let config = Config {
            max_pagination_limit: env.max_pagination_limit,
        };
        Ok(Self { pool, config })
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
            .map(|_| Uuid::new_v4().to_string())
            .collect();
        let timestamps: Vec<i64> = vec![chrono::Utc::now().timestamp(); entries.len()];
        let encrypted_data: Vec<Vec<u8>> = entries
            .iter()
            .map(|entry| entry.encrypted_data.clone())
            .collect();

        // Execute the bulk insert
        sqlx::query!(
            r#"
            INSERT INTO encrypted_data 
            (data_type, pubkey, uuid, timestamp, encrypted_data)
            SELECT 
                UNNEST($1::integer[]),
                UNNEST($2::text[]),
                UNNEST($3::text[]),
                UNNEST($4::bigint[]),
                UNNEST($5::bytea[])
            "#,
            &data_types,
            &pubkeys,
            &uuids,
            &timestamps,
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
        if uuids.len() > self.config.max_pagination_limit as usize {
            return Err(anyhow!(
                "Number of uuids exceeds the limit {}",
                self.config.max_pagination_limit
            ));
        }
        let pubkey_hex = pubkey.to_hex();
        let records = sqlx::query!(
            r#"
            SELECT uuid, timestamp, encrypted_data
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
                let meta = MetaData {
                    uuid: r.uuid,
                    timestamp: r.timestamp as u64,
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
        let actual_limit = cursor.limit.unwrap_or(self.config.max_pagination_limit) as i64;
        let cursor_meta = cursor.cursor.as_ref();
        let result = if let Some(cursor_meta) = cursor_meta {
            let records = sqlx::query!(
                r#"
                SELECT uuid, timestamp, encrypted_data
                FROM encrypted_data
                WHERE data_type = $1 AND pubkey = $2 AND (timestamp, uuid) >= ($3, $4)
                ORDER BY timestamp ASC, uuid ASC
                LIMIT $5
                "#,
                data_type as i32,
                pubkey_hex,
                cursor_meta.timestamp as i64,
                cursor_meta.uuid,
                actual_limit + 1
            )
            .fetch_all(&self.pool)
            .await?;
            let result: Vec<DataWithMetaData> = records
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
                .collect();
            result
        } else {
            let records = sqlx::query!(
                r#"
                SELECT uuid, timestamp, encrypted_data
                FROM encrypted_data
                WHERE data_type = $1 AND pubkey = $2
                ORDER BY timestamp ASC, uuid ASC
                LIMIT $3
                "#,
                data_type as i32,
                pubkey_hex,
                actual_limit + 1
            )
            .fetch_all(&self.pool)
            .await?;
            let result: Vec<DataWithMetaData> = records
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
                .collect();
            result
        };

        let result = result
            .into_iter()
            .take(actual_limit as usize)
            .collect::<Vec<DataWithMetaData>>();
        let has_more = result.len() > actual_limit as usize;
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
}

#[cfg(test)]
mod tests {
    use ethers::core::rand;
    use intmax2_interfaces::{
        data::{encryption::Encryption as _, meta_data::MetaData, user_data::UserData},
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
