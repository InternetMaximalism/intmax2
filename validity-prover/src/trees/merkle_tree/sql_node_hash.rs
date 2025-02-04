use super::{error::MerkleTreeError, HashOut, Hasher, MTResult};
use crate::trees::utils::bit_path::BitPath;
use intmax2_zkp::utils::{
    leafable::Leafable, leafable_hasher::LeafableHasher, trees::merkle_tree::MerkleProof,
};

use serde::{de::DeserializeOwned, Serialize};
use sqlx::{Pool, Postgres};

#[derive(Clone, Debug)]
pub struct SqlNodeHashes<V: Leafable + Serialize + DeserializeOwned> {
    tag: u32, // tag is used to distinguish between different trees in the same database
    height: usize,
    zero_hashes: Vec<HashOut<V>>,
    pool: Pool<Postgres>,
}

impl<V: Leafable + Serialize + DeserializeOwned> SqlNodeHashes<V> {
    pub fn new(pool: Pool<Postgres>, tag: u32, height: usize) -> Self {
        let mut zero_hashes = vec![];
        let mut h = V::empty_leaf().hash();
        zero_hashes.push(h);
        for _ in 0..height {
            let new_h = Hasher::<V>::two_to_one(h, h);
            zero_hashes.push(new_h);
            h = new_h;
        }
        zero_hashes.reverse();
        SqlNodeHashes {
            pool,
            tag,
            height,
            zero_hashes,
        }
    }

    pub fn tag(&self) -> u32 {
        self.tag
    }

    pub fn pool(&self) -> &Pool<Postgres> {
        &self.pool
    }

    pub fn height(&self) -> usize {
        self.height
    }

    pub async fn save_node(
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

    pub async fn get_sibling_hash(
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

    pub async fn get_root(
        &self,
        tx: &mut sqlx::Transaction<'_, Postgres>,
        timestamp: u64,
    ) -> MTResult<HashOut<V>> {
        let root = self
            .get_node_hash(tx, timestamp, BitPath::default())
            .await?;
        Ok(root)
    }

    pub async fn prove(
        &self,
        tx: &mut sqlx::Transaction<'_, Postgres>,
        timestamp: u64,
        index: u64,
    ) -> MTResult<MerkleProof<V>> {
        let mut path = BitPath::new(self.height as u32, index);
        path.reverse(); // path is big endian
        let mut siblings = vec![];
        while !path.is_empty() {
            siblings.push(self.get_sibling_hash(tx, timestamp, path).await?);
            path.pop();
        }
        Ok(MerkleProof { siblings })
    }

    pub async fn reset(
        &self,
        tx: &mut sqlx::Transaction<'_, Postgres>,
        timestamp: u64,
    ) -> MTResult<()> {
        sqlx::query!(
            r#"
            DELETE FROM hash_nodes
            WHERE tag = $1 AND timestamp_value >= $2
            "#,
            self.tag as i32,
            timestamp as i64
        )
        .execute(tx.as_mut())
        .await?;
        Ok(())
    }
}
