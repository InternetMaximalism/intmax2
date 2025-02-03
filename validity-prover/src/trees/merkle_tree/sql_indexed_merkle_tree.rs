use hashbrown::HashMap;
use intmax2_zkp::utils::{
    leafable::Leafable,
    leafable_hasher::LeafableHasher,
    poseidon_hash_out::PoseidonHashOut,
    trees::{indexed_merkle_tree::leaf::IndexedMerkleLeaf, merkle_tree::MerkleProof},
};
use serde::{de::DeserializeOwned, Serialize};
use sqlx::{Pool, Postgres};

use crate::trees::utils::bit_path::BitPath;

use super::{error::MerkleTreeError, HashOut, Hasher, MTResult};

type V = IndexedMerkleLeaf;

#[derive(Clone, Debug)]
pub struct SqlIndexedMerkleTree {
    tag: u32, // tag is used to distinguish between different trees in the same database
    height: usize,
    zero_hashes: Vec<PoseidonHashOut>,
    pool: Pool<Postgres>,
}

impl SqlIndexedMerkleTree {
    pub fn new(database_url: &str, tag: u32, height: usize) -> Self {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(5)
            .connect_lazy(database_url)
            .unwrap();

        let mut zero_hashes = vec![];
        let mut h = V::empty_leaf().hash();
        zero_hashes.push(h);
        for _ in 0..height {
            let new_h = Hasher::<V>::two_to_one(h, h);
            zero_hashes.push(new_h);
            h = new_h;
        }
        zero_hashes.reverse();
        SqlIndexedMerkleTree {
            pool,
            tag,
            height,
            zero_hashes,
        }
    }

    async fn save_node(
        &self,
        tx: &mut sqlx::Transaction<'_, Postgres>,
        timestamp: u64,
        bit_path: BitPath,
        hash: HashOut<V>,
    ) -> MTResult<()> {
        let bit_path = bincode::serialize(&bit_path).unwrap();
        let hash = bincode::serialize(&hash).unwrap();
        sqlx::query!(
            r#"
            INSERT INTO hash_nodes (timestamp_value, tag, bit_path, hash_value)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (timestamp_value, tag, bit_path)
            DO UPDATE SET hash_value = $4
            "#,
            timestamp as i64,
            self.tag as i32,
            bit_path,
            hash,
        )
        .execute(tx.as_mut())
        .await?;
        Ok(())
    }

    async fn get_node_hash(
        &self,
        tx: &mut sqlx::Transaction<'_, Postgres>,
        timestamp: u64,
        bit_path: BitPath,
    ) -> MTResult<HashOut<V>> {
        let bit_path_serialized = bincode::serialize(&bit_path).unwrap();
        let record = sqlx::query!(
            r#"
        SELECT hash_value 
        FROM hash_nodes 
        WHERE bit_path = $1 
          AND timestamp_value <= $2 
          AND tag = $3 
        ORDER BY timestamp_value DESC 
        LIMIT 1
        "#,
            bit_path_serialized,
            timestamp as i64,
            self.tag as i32
        )
        .fetch_optional(tx.as_mut())
        .await?;

        match record {
            Some(row) => {
                let hash = bincode::deserialize(&row.hash_value).unwrap();
                Ok(hash)
            }
            None => {
                let hash = self.zero_hashes[bit_path.len() as usize];
                Ok(hash)
            }
        }
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

        let current_len = self.get_num_leaves(tx, timestamp).await?;
        let next_len = ((position + 1) as usize).max(current_len);

        sqlx::query!(
            r#"
            INSERT INTO leaves (timestamp_value, tag, position, leaf_hash, leaf)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (timestamp_value, tag, position)
            DO UPDATE SET leaf_hash = $4, leaf = $5
            "#,
            timestamp as i64,
            self.tag as i32,
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
            self.tag as i32,
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
            self.tag as i32
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

    async fn get_leaves(&self, timestamp: u64) -> MTResult<Vec<(u64, V)>> {
        let mut tx = self.pool.begin().await?;
        let num_leaves = self.get_num_leaves(&mut tx, timestamp).await?;
        let records = sqlx::query!(
            r#"
            WITH RankedLeaves AS (
                SELECT *,
                    ROW_NUMBER() OVER (
                        PARTITION BY position 
                        ORDER BY timestamp_value DESC
                    ) as rn
                FROM leaves
                WHERE timestamp_value <= $1
                AND tag = $2
            )
            SELECT 
                timestamp_value,
                tag,
                position,
                leaf_hash,
                leaf
            FROM RankedLeaves
            WHERE rn = 1
            ORDER BY position
            "#,
            timestamp as i64,
            self.tag as i32
        )
        .fetch_all(tx.as_mut())
        .await?;
        tx.commit().await?;

        let mut leaves = HashMap::new();
        for record in records {
            let position = record.position as u64;
            let leaf: V = bincode::deserialize(&record.leaf)?;
            leaves.insert(position, leaf);
        }
        for i in 0..num_leaves {
            let i = i as u64;
            if !leaves.contains_key(&i) {
                leaves.insert(i, V::empty_leaf());
            }
        }
        let mut leaves: Vec<(u64, V)> = leaves.into_iter().collect();
        leaves.sort_by_key(|(i, _)| *i);

        Ok(leaves)
    }

    async fn get_num_leaves(
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
            self.tag as i32
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

    async fn get_sibling_hash(
        &self,
        tx: &mut sqlx::Transaction<'_, Postgres>,
        timestamp: u64,
        path: BitPath,
    ) -> MTResult<HashOut<V>> {
        if path.is_empty() {
            return Err(MerkleTreeError::WrongPathLength(0));
        }
        let sibling_path = path.sibling();
        let sibling_hash = self.get_node_hash(tx, timestamp, sibling_path).await?;
        Ok(sibling_hash)
    }

    async fn get_root(&self, timestamp: u64) -> MTResult<HashOut<V>> {
        let mut tx = self.pool.begin().await?;
        let root = self
            .get_node_hash(&mut tx, timestamp, BitPath::default())
            .await?;
        tx.commit().await?;
        Ok(root)
    }

    async fn update_leaf(&self, timestamp: u64, index: u64, leaf: V) -> super::MTResult<()> {
        let mut path = BitPath::new(self.height as u32, index);
        path.reverse();
        let mut h = leaf.hash();

        let mut tx = self.pool.begin().await?;
        self.save_leaf(&mut tx, timestamp, index, leaf).await?;
        self.save_node(&mut tx, timestamp, path, h).await?;
        while !path.is_empty() {
            let sibling = self.get_sibling_hash(&mut tx, timestamp, path).await?;
            let b = path.pop().unwrap(); // safe to unwrap
            let new_h = if b {
                Hasher::<V>::two_to_one(sibling, h)
            } else {
                Hasher::<V>::two_to_one(h, sibling)
            };
            self.save_node(&mut tx, timestamp, path, new_h).await?;
            h = new_h;
        }
        tx.commit().await?;
        Ok(())
    }

    async fn prove(&self, timestamp: u64, index: u64) -> MTResult<MerkleProof<V>> {
        let mut path = BitPath::new(self.height as u32, index);
        path.reverse(); // path is big endian
        let mut siblings = vec![];
        let mut tx = self.pool.begin().await?;
        while !path.is_empty() {
            siblings.push(self.get_sibling_hash(&mut tx, timestamp, path).await?);
            path.pop();
        }
        tx.commit().await?;
        Ok(MerkleProof { siblings })
    }

    async fn reset(&self) -> MTResult<()> {
        sqlx::query!(
            r#"
            DELETE FROM hash_nodes
            WHERE tag = $1
            "#,
            self.tag as i32
        )
        .execute(&self.pool)
        .await?;

        sqlx::query!(
            r#"
            DELETE FROM leaves
            WHERE tag = $1
            "#,
            self.tag as i32
        )
        .execute(&self.pool)
        .await?;

        sqlx::query!(
            r#"
            DELETE FROM leaves_len
            WHERE tag = $1
            "#,
            self.tag as i32
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn get_last_timestamp(&self) -> u64 {
        let record = sqlx::query!(
            r#"
            SELECT timestamp_value
            FROM leaves
            WHERE tag = $1
            ORDER BY timestamp_value DESC
            LIMIT 1
            "#,
            self.tag as i32
        )
        .fetch_optional(&self.pool)
        .await
        .unwrap();
        match record {
            Some(row) => row.timestamp_value as u64,
            None => 0,
        }
    }
}

use crate::trees::merkle_tree::MerkleTreeClient;

#[async_trait::async_trait(?Send)]
impl MerkleTreeClient<V> for SqlIndexedMerkleTree {
    async fn update_leaf(&self, timestamp: u64, position: u64, leaf: V) -> MTResult<()> {
        self.update_leaf(timestamp, position, leaf).await?;
        Ok(())
    }

    async fn get_root(&self, timestamp: u64) -> MTResult<HashOut<V>> {
        self.get_root(timestamp).await
    }

    async fn get_leaf(&self, timestamp: u64, position: u64) -> MTResult<V> {
        let mut tx = self.pool.begin().await?;
        let leaf = self.get_leaf(&mut tx, timestamp, position).await?;
        tx.commit().await?;
        Ok(leaf)
    }

    async fn get_leaves(&self, timestamp: u64) -> MTResult<Vec<V>> {
        let leaves = self.get_leaves(timestamp).await?;
        Ok(leaves.into_iter().map(|(_, leaf)| leaf).collect())
    }

    async fn get_num_leaves(&self, timestamp: u64) -> MTResult<usize> {
        let mut tx = self.pool.begin().await?;
        let len = self.get_num_leaves(&mut tx, timestamp).await?;
        tx.commit().await?;
        Ok(len)
    }

    async fn prove(&self, timestamp: u64, position: u64) -> MTResult<MerkleProof<V>> {
        self.prove(timestamp, position).await
    }

    async fn reset(&self) -> MTResult<()> {
        self.reset().await
    }

    fn height(&self) -> usize {
        self.height
    }

    async fn get_last_timestamp(&self) -> MTResult<u64> {
        Ok(self.get_last_timestamp().await)
    }
}
