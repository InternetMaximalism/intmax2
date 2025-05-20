use intmax2_zkp::utils::{
    leafable::Leafable,
    leafable_hasher::LeafableHasher,
    trees::{bit_path::BitPath, incremental_merkle_tree::IncrementalMerkleProof},
};
use serde::{de::DeserializeOwned, Serialize};
use sqlx::{Pool, Postgres};

use super::{sql_node_hash::SqlNodeHashes, HashOut, Hasher, IncrementalMerkleTreeClient, MTResult};

#[derive(Clone, Debug)]
pub struct SqlIncrementalMerkleTree<V: Leafable + Serialize + DeserializeOwned> {
    sql_node_hashes: SqlNodeHashes<V>,
}

impl<V: Leafable + Serialize + DeserializeOwned> SqlIncrementalMerkleTree<V> {
    pub fn new(pool: Pool<Postgres>, tag: u32, height: usize) -> Self {
        let sql_node_hashes = SqlNodeHashes::new(pool, tag, height);
        SqlIncrementalMerkleTree { sql_node_hashes }
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
        let leaf_serialized = bincode::serialize(&leaf).unwrap();

        let current_len = self.len(tx, timestamp).await?;
        let next_len = ((position + 1) as usize).max(current_len);

        sqlx::query!(
            r#"
            INSERT INTO leaves (timestamp_value, tag, position, leaf_hash, leaf)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (timestamp_value, tag, position)
            DO UPDATE SET leaf_hash = $4, leaf = $5
            "#,
            timestamp as i64,
            self.tag() as i32,
            position as i64,
            leaf_hash_serialized,
            leaf_serialized,
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
        SELECT leaf 
        FROM leaves 
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
                let leaf = bincode::deserialize(&row.leaf)?;
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

    async fn push(
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
            DELETE FROM leaves
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
            FROM leaves
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

    async fn prove(
        &self,
        tx: &mut sqlx::Transaction<'_, Postgres>,
        timestamp: u64,
        index: u64,
    ) -> MTResult<IncrementalMerkleProof<V>> {
        let proof = self.sql_node_hashes.prove(tx, timestamp, index).await?;
        Ok(IncrementalMerkleProof(proof))
    }
}

#[async_trait::async_trait(?Send)]
impl<V: Leafable + Serialize + DeserializeOwned> IncrementalMerkleTreeClient<V>
    for SqlIncrementalMerkleTree<V>
{
    fn height(&self) -> usize {
        self.height()
    }

    async fn get_root(&self, timestamp: u64) -> MTResult<HashOut<V>> {
        let mut tx = self.pool().begin().await?;
        let root = self.sql_node_hashes.get_root(&mut tx, timestamp).await?;
        tx.commit().await?;
        Ok(root)
    }

    async fn get_leaf(&self, timestamp: u64, position: u64) -> MTResult<V> {
        let mut tx = self.pool().begin().await?;
        let leaf = self.get_leaf(&mut tx, timestamp, position).await?;
        tx.commit().await?;
        Ok(leaf)
    }

    async fn len(&self, timestamp: u64) -> MTResult<usize> {
        let mut tx = self.pool().begin().await?;
        let len = self.len(&mut tx, timestamp).await?;
        tx.commit().await?;
        Ok(len)
    }

    async fn update_leaf(&self, timestamp: u64, position: u64, leaf: V) -> MTResult<()> {
        let mut tx = self.pool().begin().await?;
        self.update_leaf(&mut tx, timestamp, position, leaf).await?;
        tx.commit().await?;
        Ok(())
    }

    async fn push(&self, timestamp: u64, leaf: V) -> MTResult<()> {
        let mut tx = self.pool().begin().await?;
        self.push(&mut tx, timestamp, leaf).await?;
        tx.commit().await?;
        Ok(())
    }

    async fn prove(&self, timestamp: u64, position: u64) -> MTResult<IncrementalMerkleProof<V>> {
        let mut tx = self.pool().begin().await?;
        let proof = self.prove(&mut tx, timestamp, position).await?;
        tx.commit().await?;
        Ok(proof)
    }

    async fn get_last_timestamp(&self) -> MTResult<u64> {
        let mut tx = self.pool().begin().await?;
        let timestamp = self.get_last_timestamp(&mut tx).await;
        tx.commit().await?;
        Ok(timestamp)
    }

    async fn reset(&self, timestamp: u64) -> MTResult<()> {
        let mut tx = self.pool().begin().await?;
        self.reset(&mut tx, timestamp).await?;
        tx.commit().await?;
        Ok(())
    }
}
