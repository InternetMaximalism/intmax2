use std::str::FromStr;

use bigdecimal::{num_bigint::BigUint, BigDecimal};
use intmax2_zkp::{
    ethereum_types::{u256::U256, u32limb_trait::U32LimbTrait},
    utils::{
        leafable::Leafable,
        leafable_hasher::LeafableHasher,
        poseidon_hash_out::PoseidonHashOut,
        trees::{indexed_merkle_tree::leaf::IndexedMerkleLeaf, merkle_tree::MerkleProof},
    },
};
use sqlx::{Pool, Postgres};

use crate::trees::utils::bit_path::BitPath;

use super::{
    error::MerkleTreeError, sql_node_hash::SqlNodeHashes, HashOut, Hasher, IndexedMerkleTreeClient,
    MTResult,
};

type V = IndexedMerkleLeaf;

// next_index bigint NOT NULL,
// key NUMERIC(78, 0) NOT NULL,
// next_key NUMERIC(78, 0) NOT NULL,
// value bigint NOT NULL,

#[derive(Clone, Debug)]
pub struct SqlIndexedMerkleTree {
    sql_node_hashes: SqlNodeHashes<V>,
}

impl SqlIndexedMerkleTree {
    pub fn new(database_url: &str, tag: u32, height: usize) -> Self {
        let sql_node_hashes = SqlNodeHashes::new(database_url, tag, height);
        SqlIndexedMerkleTree { sql_node_hashes }
    }

    pub fn tag(&self) -> u32 {
        self.sql_node_hashes.tag()
    }

    pub fn pool(&self) -> &Pool<Postgres> {
        self.sql_node_hashes.pool()
    }

    pub fn height(&self) -> usize {
        self.sql_node_hashes.height()
    }

    async fn save_leaf(
        &self,
        tx: &mut sqlx::Transaction<'_, Postgres>,
        timestamp: u64,
        position: u64,
        leaf: V,
    ) -> super::MTResult<()> {
        let leaf_hash_serialized = bincode::serialize(&leaf.hash()).unwrap();
        let current_len = self.len(tx, timestamp).await?;
        let next_len = ((position + 1) as usize).max(current_len);

        let key = BigDecimal::from_str(&leaf.key.to_string()).unwrap();
        let next_key = BigDecimal::from_str(&leaf.next_key.to_string()).unwrap();
        sqlx::query!(
            r#"
            INSERT INTO indexed_leaves (timestamp_value, tag, position, leaf_hash, next_index, key, next_key, value)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (timestamp_value, tag, position)
            DO UPDATE SET leaf_hash = $4, next_index = $5, key = $6, next_key = $7, value = $8
            "#,
            timestamp as i64,
            self.tag() as i32,
            position as i64,
            leaf_hash_serialized,
            leaf.next_index as i64,
            key,
            next_key,
            leaf.value as i64,
        )
        .execute(tx.as_mut())
        .await?;
        sqlx::query!(
            r#"
            INSERT INTO leaves_len (timestamp_value, tag, len)
            VALUES ($1, $2, $3)
            ON CONFLICT (timestamp_value, tag)
            DO UPDATE SET len = $3
            "#,
            timestamp as i64,
            self.tag() as i32,
            next_len as i32,
        )
        .execute(tx.as_mut())
        .await?;

        Ok(())
    }

    async fn get_leaf(
        &self,
        tx: &mut sqlx::Transaction<'_, Postgres>,
        timestamp: u64,
        position: u64,
    ) -> super::MTResult<V> {
        let record = sqlx::query!(
            r#"
        SELECT next_index, key, next_key, value
        FROM indexed_leaves
        WHERE position = $1 
          AND timestamp_value <= $2 
          AND tag = $3 
        ORDER BY timestamp_value DESC 
        LIMIT 1
        "#,
            position as i64,
            timestamp as i64,
            self.tag() as i32
        )
        .fetch_optional(tx.as_mut())
        .await?;

        match record {
            Some(row) => {
                let next_index = row.next_index as u64;
                let key = from_str_to_u256(&row.key.to_string());
                let next_key = from_str_to_u256(&row.next_key.to_string());
                let value = row.value as u64;
                let leaf = IndexedMerkleLeaf {
                    next_index,
                    key,
                    next_key,
                    value,
                };
                Ok(leaf)
            }
            None => Ok(V::empty_leaf()),
        }
    }

    async fn update_leaf(
        &self,
        tx: &mut sqlx::Transaction<'_, Postgres>,
        timestamp: u64,
        index: u64,
        leaf: V,
    ) -> super::MTResult<()> {
        let mut path = BitPath::new(self.height() as u32, index);
        path.reverse();
        let mut h = leaf.hash();
        self.save_leaf(tx, timestamp, index, leaf).await?;
        self.sql_node_hashes
            .save_node(tx, timestamp, path, h)
            .await?;
        while !path.is_empty() {
            let sibling = self
                .sql_node_hashes
                .get_sibling_hash(tx, timestamp, path)
                .await?;
            let b = path.pop().unwrap(); // safe to unwrap
            let new_h = if b {
                Hasher::<V>::two_to_one(sibling, h)
            } else {
                Hasher::<V>::two_to_one(h, sibling)
            };
            self.sql_node_hashes
                .save_node(tx, timestamp, path, new_h)
                .await?;
            h = new_h;
        }
        Ok(())
    }

    async fn len(
        &self,
        tx: &mut sqlx::Transaction<'_, Postgres>,
        timestamp: u64,
    ) -> super::MTResult<usize> {
        let record = sqlx::query!(
            r#"
            SELECT len
            FROM leaves_len
            WHERE timestamp_value <= $1
              AND tag = $2
            ORDER BY timestamp_value DESC
            LIMIT 1
            "#,
            timestamp as i64,
            self.tag() as i32
        )
        .fetch_optional(tx.as_mut())
        .await?;

        match record {
            Some(row) => {
                let len = row.len as usize;
                Ok(len)
            }
            None => Ok(0),
        }
    }

    pub async fn push(
        &self,
        tx: &mut sqlx::Transaction<'_, Postgres>,
        timestamp: u64,
        leaf: V,
    ) -> MTResult<()> {
        let index = self.len(tx, timestamp).await? as u64;
        self.update_leaf(tx, timestamp, index, leaf).await?;
        Ok(())
    }

    async fn reset(
        &self,
        tx: &mut sqlx::Transaction<'_, Postgres>,
        timestamp: u64,
    ) -> MTResult<()> {
        self.sql_node_hashes.reset(tx, timestamp).await?;
        sqlx::query!(
            r#"
            DELETE FROM indexed_leaves
            WHERE tag = $1 AND timestamp_value >= $2
            "#,
            self.tag() as i32,
            timestamp as i64
        )
        .execute(tx.as_mut())
        .await?;
        sqlx::query!(
            r#"
            DELETE FROM leaves_len
            WHERE tag = $1 AND timestamp_value >= $2
            "#,
            self.tag() as i32,
            timestamp as i64
        )
        .execute(tx.as_mut())
        .await?;

        Ok(())
    }

    async fn get_last_timestamp(&self, tx: &mut sqlx::Transaction<'_, Postgres>) -> u64 {
        let record = sqlx::query!(
            r#"
            SELECT timestamp_value
            FROM indexed_leaves
            WHERE tag = $1
            ORDER BY timestamp_value DESC
            LIMIT 1
            "#,
            self.tag() as i32
        )
        .fetch_optional(tx.as_mut())
        .await
        .unwrap();
        match record {
            Some(row) => row.timestamp_value as u64,
            None => 0,
        }
    }

    pub async fn low_index(
        &self,
        tx: &mut sqlx::Transaction<'_, Postgres>,
        timestamp: u64,
        key: U256,
    ) -> MTResult<u64> {
        let key_decimal = BigDecimal::from_str(&key.to_string()).unwrap();
        let rows = sqlx::query!(
            r#"
            WITH latest_leaves AS (
                SELECT DISTINCT ON (position) position, key, next_key
                FROM indexed_leaves
                WHERE timestamp_value <= $1 AND tag = $2
                ORDER BY position, timestamp_value DESC
            )
            SELECT position
            FROM latest_leaves
            WHERE key < $3 AND ($3 < next_key OR next_key = '0'::numeric)
            "#,
            timestamp as i64,
            self.tag() as i32,
            key_decimal
        )
        .fetch_all(tx.as_mut())
        .await?;

        if rows.is_empty() {
            return Err(MerkleTreeError::InternalError(
                "key already exists".to_string(),
            ));
        }
        if rows.len() > 1 {
            return Err(MerkleTreeError::InternalError(
                "low_index: too many candidates".to_string(),
            ));
        }
        Ok(rows[0].position as u64)
    }

    pub async fn index(
        &self,
        tx: &mut sqlx::Transaction<'_, Postgres>,
        timestamp: u64,
        key: U256,
    ) -> MTResult<Option<u64>> {
        let key_decimal = BigDecimal::from_str(&key.to_string())
            .map_err(|e| MerkleTreeError::InternalError(e.to_string()))?;
        let rows = sqlx::query!(
            r#"
            WITH latest_leaves AS (
                SELECT DISTINCT ON (position) position, key
                FROM indexed_leaves
                WHERE timestamp_value <= $1 AND tag = $2
                ORDER BY position, timestamp_value DESC
            )
            SELECT position
            FROM latest_leaves
            WHERE key = $3
            "#,
            timestamp as i64,
            self.tag() as i32,
            key_decimal
        )
        .fetch_all(tx.as_mut())
        .await?;

        if rows.is_empty() {
            Ok(None)
        } else if rows.len() > 1 {
            Err(MerkleTreeError::InternalError(
                "find_index: too many candidates".to_string(),
            ))
        } else {
            Ok(Some(rows[0].position as u64))
        }
    }

    pub async fn key(
        &self,
        tx: &mut sqlx::Transaction<'_, Postgres>,
        timestamp: u64,
        index: u64,
    ) -> MTResult<U256> {
        let rec = sqlx::query!(
            r#"
            WITH latest_leaves AS (
                SELECT DISTINCT ON (position) position, key
                FROM indexed_leaves
                WHERE timestamp_value <= $1 AND tag = $2
                ORDER BY position, timestamp_value DESC
            )
            SELECT key
            FROM latest_leaves
            WHERE position = $3
            "#,
            timestamp as i64,
            self.tag() as i32,
            index as i64
        )
        .fetch_optional(tx.as_mut())
        .await?;
        if let Some(row) = rec {
            Ok(from_str_to_u256(&row.key.to_string()))
        } else {
            Ok(U256::default())
        }
    }


    
}

fn from_str_to_u256(s: &str) -> U256 {
    U256::from_bytes_be(&BigUint::from_str(s).unwrap().to_bytes_be())
}

// #[async_trait(?Send)]
// pub trait IndexedMerkleTreeClient: std::fmt::Debug + Clone {
//     async fn get_root(&self, timestamp: u64) -> MTResult<PoseidonHashOut>;
//     async fn get_leaf(&self, timestamp: u64, index: u64) -> MTResult<IndexedMerkleLeaf>;
//     async fn prove(&self, timestamp: u64, index: u64) -> MTResult<IndexedMerkleProof>;
//     async fn low_index(&self, timestamp: u64, key: U256) -> MTResult<u64>;
//     async fn index(&self, timestamp: u64, key: U256) -> MTResult<Option<u64>>;
//     async fn key(&self, timestamp: u64, index: u64) -> MTResult<U256>;
//     async fn update(&self, timestamp: u64, key: U256, value: u64) -> MTResult<()>;
//     async fn len(&self, timestamp: u64) -> MTResult<usize>;
// }

// #[async_trait::async_trait(?Send)]
// impl IndexedMerkleTreeClient for SqlIndexedMerkleTree {
//     async fn get_root(&self, timestamp: u64) -> MTResult<PoseidonHashOut> {
//         self.get_root(timestamp).await
//     }

//     async fn get_leaf(&self, timestamp: u64, index: u64) -> MTResult<IndexedMerkleLeaf> {
//         let mut tx = self.pool.begin().await?;
//         let leaf = self.get_leaf(&mut tx, timestamp, index).await?;
//         tx.commit().await?;
//         Ok(leaf)
//     }

//     async fn prove(&self, timestamp: u64, index: u64) -> MTResult<MerkleProof<V>> {
//         self.prove(timestamp, index).await
//     }

//     async fn low_index(&self, timestamp: u64, key: U256) -> MTResult<u64> {
//         let mut tx = self.pool.begin().await?;
//         let low_index = self.low_index(&mut tx, timestamp, key).await?;
//         tx.commit().await?;
//         Ok(low_index)
//     }

//     async fn index(&self, timestamp: u64, key: U256) -> MTResult<Option<u64>> {
//         let mut tx = self.pool.begin().await?;
//         let index = self.index(&mut tx, timestamp, key).await?;
//         tx.commit().await?;
//         Ok(index)
//     }

//     async fn key(&self, timestamp: u64, index: u64) -> MTResult<U256> {
//         self.key(timestamp, index).await
//     }

//     async fn update(&self, timestamp: u64, key: U256, value: u64) -> MTResult<()> {
//         let mut tx = self.pool.begin().await?;
//         self.update_leaf(
//             &mut tx,
//             timestamp,
//             key.as_u64(),
//             IndexedMerkleLeaf::new(key, value),
//         )
//         .await?;
//         tx.commit().await?;
//         Ok(())
//     }

//     async fn reset(&self, timestamp: u64) -> MTResult<()> {
//         self.reset(timestamp).await
//     }

//     async fn len(&self, timestamp: u64) -> MTResult<usize> {
//         let mut tx = self.pool.begin().await?;
//         let len = self.get_num_leaves(&mut tx, timestamp).await?;
//         tx.commit().await?;
//         Ok(len)
//     }
// }
