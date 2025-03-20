use std::str::FromStr;

use bigdecimal::{num_bigint::BigUint, BigDecimal};
use intmax2_zkp::{
    common::trees::account_tree::AccountMerkleProof,
    ethereum_types::u256::U256,
    utils::{
        leafable::Leafable,
        leafable_hasher::LeafableHasher,
        trees::{
            incremental_merkle_tree::IncrementalMerkleProof,
            indexed_merkle_tree::{
                insertion::IndexedInsertionProof, leaf::IndexedMerkleLeaf,
                membership::MembershipProof, update::UpdateProof, IndexedMerkleProof,
            },
        },
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
    pub fn new(pool: Pool<Postgres>, tag: u32, height: usize) -> Self {
        let sql_node_hashes = SqlNodeHashes::new(pool, tag, height);
        SqlIndexedMerkleTree { sql_node_hashes }
    }

    // add default leaf to the first position of the tree
    pub async fn initialize(&self) -> MTResult<()> {
        let mut tx = self.pool().begin().await?;
        let last_timestamp = self.get_last_timestamp(&mut tx).await;
        if last_timestamp == 0 && self.len(&mut tx, last_timestamp).await? == 0 {
            self.push(&mut tx, last_timestamp, V::default()).await?;
            if self.len(&mut tx, last_timestamp).await? == 1 {
                self.insert(&mut tx, last_timestamp, U256::dummy_pubkey(), 0)
                    .await?; // add default account
            }
        }
        tx.commit().await?;
        Ok(())
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
        let leaf_hash = leaf.hash();
        self.save_leaf(tx, timestamp, index, leaf).await?;
        self.sql_node_hashes
            .save_node(tx, timestamp, path, leaf_hash)
            .await?;

        // collect paths
        let mut paths = Vec::new();
        let mut current_path = path;
        while !current_path.is_empty() {
            paths.push(current_path);
            current_path.pop();
        }

        if !paths.is_empty() {
            let sibling_hashes = self
                .sql_node_hashes
                .bulk_get_sibling_hashes(tx, timestamp, &paths)
                .await?;

            let mut h = leaf_hash;
            let mut current_path = path;

            for sibling in sibling_hashes {
                let bit = current_path.pop().unwrap();
                let new_h = if bit {
                    Hasher::<V>::two_to_one(sibling, h)
                } else {
                    Hasher::<V>::two_to_one(h, sibling)
                };
                self.sql_node_hashes
                    .save_node(tx, timestamp, current_path, new_h)
                    .await?;
                h = new_h;
            }
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

    async fn low_index(
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

    async fn index(
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

    async fn key(
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

    async fn prove(
        &self,
        tx: &mut sqlx::Transaction<'_, Postgres>,
        timestamp: u64,
        index: u64,
    ) -> MTResult<IndexedMerkleProof> {
        let proof = self.sql_node_hashes.prove(tx, timestamp, index).await?;
        Ok(IncrementalMerkleProof(proof))
    }

    async fn prove_membership(
        &self,
        tx: &mut sqlx::Transaction<'_, Postgres>,
        timestamp: u64,
        key: U256,
    ) -> MTResult<MembershipProof> {
        if let Some(index) = self.index(tx, timestamp, key).await? {
            // inclusion proof
            Ok(MembershipProof {
                is_included: true,
                leaf_index: index,
                leaf: self.get_leaf(tx, timestamp, index).await?,
                leaf_proof: self.prove(tx, timestamp, index).await?,
            })
        } else {
            // exclusion proof
            let low_index = self.low_index(tx, timestamp, key).await?; // unwrap is safe here
            Ok(MembershipProof {
                is_included: false,
                leaf_index: low_index,
                leaf: self.get_leaf(tx, timestamp, low_index).await?,
                leaf_proof: self.prove(tx, timestamp, low_index).await?,
            })
        }
    }

    async fn prove_inclusion(
        &self,
        tx: &mut sqlx::Transaction<'_, Postgres>,
        timestamp: u64,
        account_id: u64,
    ) -> MTResult<AccountMerkleProof> {
        let leaf = self.get_leaf(tx, timestamp, account_id).await?;
        let merkle_proof = self.prove(tx, timestamp, account_id).await?;
        Ok(AccountMerkleProof { merkle_proof, leaf })
    }

    async fn insert(
        &self,
        tx: &mut sqlx::Transaction<'_, Postgres>,
        timestamp: u64,
        key: U256,
        value: u64,
    ) -> MTResult<()> {
        let index = self.len(tx, timestamp).await? as u64;
        let low_index = self.low_index(tx, timestamp, key).await?;
        let prev_low_leaf = self.get_leaf(tx, timestamp, low_index).await?;
        let new_low_leaf = IndexedMerkleLeaf {
            next_index: index,
            next_key: key,
            ..prev_low_leaf
        };
        let leaf = IndexedMerkleLeaf {
            next_index: prev_low_leaf.next_index,
            key,
            next_key: prev_low_leaf.next_key,
            value,
        };
        self.update_leaf(tx, timestamp, low_index, new_low_leaf)
            .await?;
        self.push(tx, timestamp, leaf).await?;
        Ok(())
    }

    async fn prove_and_insert(
        &self,
        tx: &mut sqlx::Transaction<'_, Postgres>,
        timestamp: u64,
        key: U256,
        value: u64,
    ) -> MTResult<IndexedInsertionProof> {
        let index = self.len(tx, timestamp).await? as u64;
        let low_index = self.low_index(tx, timestamp, key).await?;
        let prev_low_leaf = self.get_leaf(tx, timestamp, low_index).await?;
        let new_low_leaf = IndexedMerkleLeaf {
            next_index: index,
            next_key: key,
            ..prev_low_leaf
        };
        let leaf = IndexedMerkleLeaf {
            next_index: prev_low_leaf.next_index,
            key,
            next_key: prev_low_leaf.next_key,
            value,
        };
        let low_leaf_proof = self.prove(tx, timestamp, low_index).await?;
        self.update_leaf(tx, timestamp, low_index, new_low_leaf)
            .await?;
        self.push(tx, timestamp, leaf).await?;
        let leaf_proof = self.prove(tx, timestamp, index).await?;
        Ok(IndexedInsertionProof {
            index,
            low_leaf_proof,
            leaf_proof,
            low_leaf_index: low_index,
            prev_low_leaf,
        })
    }

    async fn prove_and_update(
        &self,
        tx: &mut sqlx::Transaction<'_, Postgres>,
        timestamp: u64,
        key: U256,
        new_value: u64,
    ) -> MTResult<UpdateProof> {
        let index = self
            .index(tx, timestamp, key)
            .await?
            .ok_or_else(|| MerkleTreeError::InternalError("key not found".to_string()))?;
        let prev_leaf = self.get_leaf(tx, timestamp, index).await?;
        let new_leaf = IndexedMerkleLeaf {
            value: new_value,
            ..prev_leaf
        };
        self.update_leaf(tx, timestamp, index, new_leaf).await?;
        Ok(UpdateProof {
            leaf_proof: self.prove(tx, timestamp, index).await?,
            leaf_index: index,
            prev_leaf,
        })
    }
}

fn from_str_to_u256(s: &str) -> U256 {
    let biguint = BigUint::from_str(s).unwrap();
    biguint.try_into().unwrap()
}

#[async_trait::async_trait(?Send)]
impl IndexedMerkleTreeClient for SqlIndexedMerkleTree {
    async fn get_root(&self, timestamp: u64) -> MTResult<HashOut<V>> {
        let mut tx = self.pool().begin().await?;
        let root = self.sql_node_hashes.get_root(&mut tx, timestamp).await?;
        tx.commit().await?;
        Ok(root)
    }

    async fn get_leaf(&self, timestamp: u64, index: u64) -> MTResult<V> {
        let mut tx = self.pool().begin().await?;
        let leaf = self.get_leaf(&mut tx, timestamp, index).await?;
        tx.commit().await?;
        Ok(leaf)
    }

    async fn len(&self, timestamp: u64) -> MTResult<usize> {
        let mut tx = self.pool().begin().await?;
        let len = self.len(&mut tx, timestamp).await?;
        tx.commit().await?;
        Ok(len)
    }

    async fn push(&self, timestamp: u64, leaf: V) -> MTResult<()> {
        let mut tx = self.pool().begin().await?;
        self.push(&mut tx, timestamp, leaf).await?;
        tx.commit().await?;
        Ok(())
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

    async fn key(&self, timestamp: u64, index: u64) -> MTResult<U256> {
        let mut tx = self.pool().begin().await?;
        let key = self.key(&mut tx, timestamp, index).await?;
        tx.commit().await?;
        Ok(key)
    }

    async fn index(&self, timestamp: u64, key: U256) -> MTResult<Option<u64>> {
        let mut tx = self.pool().begin().await?;
        let index = self.index(&mut tx, timestamp, key).await?;
        tx.commit().await?;
        Ok(index)
    }

    async fn prove_inclusion(
        &self,
        timestamp: u64,
        account_id: u64,
    ) -> MTResult<AccountMerkleProof> {
        let mut tx = self.pool().begin().await?;
        let proof = self.prove_inclusion(&mut tx, timestamp, account_id).await;
        tx.commit().await?;
        proof
    }

    async fn prove_membership(&self, timestamp: u64, key: U256) -> MTResult<MembershipProof> {
        let mut tx = self.pool().begin().await?;
        let proof = self.prove_membership(&mut tx, timestamp, key).await;
        tx.commit().await?;
        proof
    }

    async fn insert(&self, timestamp: u64, key: U256, value: u64) -> MTResult<()> {
        let mut tx = self.pool().begin().await?;
        self.insert(&mut tx, timestamp, key, value).await?;
        tx.commit().await?;
        Ok(())
    }

    async fn prove_and_insert(
        &self,
        timestamp: u64,
        key: U256,
        value: u64,
    ) -> MTResult<IndexedInsertionProof> {
        let mut tx = self.pool().begin().await?;
        let proof = self.prove_and_insert(&mut tx, timestamp, key, value).await;
        tx.commit().await?;
        proof
    }

    async fn prove_and_update(
        &self,
        timestamp: u64,
        key: U256,
        new_value: u64,
    ) -> MTResult<UpdateProof> {
        let mut tx = self.pool().begin().await?;
        let proof = self
            .prove_and_update(&mut tx, timestamp, key, new_value)
            .await;
        tx.commit().await?;
        proof
    }
}

#[cfg(test)]
mod tests {
    use super::IndexedMerkleTreeClient;
    use crate::trees::merkle_tree::sql_indexed_merkle_tree::{
        from_str_to_u256, SqlIndexedMerkleTree,
    };
    use intmax2_zkp::{
        common::trees::account_tree::AccountTree, constants::ACCOUNT_TREE_HEIGHT,
        ethereum_types::u256::U256, utils::trees::indexed_merkle_tree::leaf::IndexedMerkleLeaf,
    };

    fn setup_test() -> String {
        dotenv::dotenv().ok();
        std::env::var("DATABASE_URL").unwrap()
    }

    #[tokio::test]
    async fn test_account_tree() -> anyhow::Result<()> {
        let database_url = setup_test();

        let tag = 3;
        let pool = sqlx::Pool::connect(&database_url).await?;
        let tree = SqlIndexedMerkleTree::new(pool, tag, ACCOUNT_TREE_HEIGHT);
        <SqlIndexedMerkleTree as IndexedMerkleTreeClient>::reset(&tree, 0).await?;

        tree.initialize().await?;

        let timestamp0 = 0;
        let mut tx = tree.pool().begin().await?;
        for i in 2..5 {
            tree.insert(&mut tx, timestamp0, i.into(), i.into()).await?;
        }
        let old_root = tree.sql_node_hashes.get_root(&mut tx, timestamp0).await?;

        let timestamp1 = 1;

        for i in 5..8 {
            tree.insert(&mut tx, timestamp1, i.into(), i.into()).await?;
        }
        tx.commit().await?;

        let account_id = 3;
        let mut tx = tree.pool().begin().await?;
        let proof = tree
            .prove_inclusion(&mut tx, timestamp0, account_id)
            .await?;
        tx.commit().await?;

        let result = proof.verify(old_root, account_id, (account_id as u32).into());
        assert!(result);

        Ok(())
    }

    #[tokio::test]
    async fn test_comparison_account_tree() -> anyhow::Result<()> {
        let database_url = setup_test();
        let tag = 4;
        let pool = sqlx::Pool::connect(&database_url).await?;
        let db_tree = SqlIndexedMerkleTree::new(pool, tag, ACCOUNT_TREE_HEIGHT);
        <SqlIndexedMerkleTree as IndexedMerkleTreeClient>::reset(&db_tree, 0).await?;

        db_tree.initialize().await?;

        let mut tx = db_tree.pool().begin().await?;
        let timestamp = db_tree.get_last_timestamp(&mut tx).await;
        for i in 2..10 {
            db_tree
                .insert(&mut tx, timestamp, i.into(), i.into())
                .await?;
        }
        let db_root = db_tree.sql_node_hashes.get_root(&mut tx, timestamp).await?;

        let mut tree = AccountTree::initialize();
        for i in 2..10 {
            tree.insert(i.into(), i.into())?;
        }
        let root = tree.get_root();
        assert_eq!(db_root, root);
        Ok(())
    }

    #[tokio::test]
    async fn test_update_leaf_consistency() -> anyhow::Result<()> {
        let database_url = setup_test();
        let tag = 1000;
        let pool = sqlx::Pool::connect(&database_url).await?;
        let tree = SqlIndexedMerkleTree::new(pool, tag, ACCOUNT_TREE_HEIGHT);
        <SqlIndexedMerkleTree as IndexedMerkleTreeClient>::reset(&tree, 0).await?;

        tree.initialize().await?;
        let timestamp = 123;

        let test_leaves = vec![
            (1, "100", "200", 1000),
            (2, "200", "300", 2000),
            (3, "300", "400", 3000),
            (10, "1000", "1100", 10000),
            (20, "2000", "2100", 20000),
        ];

        for &(index, key, next_key, value) in &test_leaves {
            let mut tx = tree.pool().begin().await?;

            let leaf = IndexedMerkleLeaf {
                next_index: index + 1,
                key: from_str_to_u256(key),
                next_key: from_str_to_u256(next_key),
                value,
            };

            tree.update_leaf(&mut tx, timestamp, index, leaf.clone())
                .await?;
            let stored_leaf = tree.get_leaf(&mut tx, timestamp, index).await?;
            assert_eq!(
                stored_leaf.key, leaf.key,
                "Key should match for index {}",
                index
            );
            assert_eq!(
                stored_leaf.next_key, leaf.next_key,
                "NextKey should match for index {}",
                index
            );
            assert_eq!(
                stored_leaf.value, leaf.value,
                "Value should match for index {}",
                index
            );

            tx.commit().await?;
        }

        {
            let mut tx = tree.pool().begin().await?;

            for &(index, key, _, value) in &test_leaves {
                let leaf = tree.get_leaf(&mut tx, timestamp, index).await?;
                assert_eq!(
                    leaf.key,
                    from_str_to_u256(key),
                    "Key should match for retrieved leaf at index {}",
                    index
                );
                assert_eq!(
                    leaf.value, value,
                    "Value should match for retrieved leaf at index {}",
                    index
                );
            }

            for &(index, key, _, __) in &test_leaves {
                let proof = tree.prove_inclusion(&mut tx, timestamp, index).await?;
                let root = tree.sql_node_hashes.get_root(&mut tx, timestamp).await?;

                let result = proof.verify(root, index, from_str_to_u256(key));
                assert!(result, "Proof verification failed for index {}", index);
            }

            tx.rollback().await?;
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_update_leaf_edge_cases() -> anyhow::Result<()> {
        let database_url = setup_test();
        let tag = 2000;
        let pool = sqlx::Pool::connect(&database_url).await?;
        let tree = SqlIndexedMerkleTree::new(pool, tag, ACCOUNT_TREE_HEIGHT);
        <SqlIndexedMerkleTree as IndexedMerkleTreeClient>::reset(&tree, 0).await?;

        tree.initialize().await?;
        let timestamp = 456;
        let mut tx = tree.pool().begin().await?;

        let leaf1 = IndexedMerkleLeaf {
            next_index: 2,
            key: U256::from(1),
            next_key: U256::from(2),
            value: 100,
        };
        tree.update_leaf(&mut tx, timestamp, 1, leaf1.clone())
            .await?;
        let stored_leaf1 = tree.get_leaf(&mut tx, timestamp, 1).await?;
        assert_eq!(
            stored_leaf1.key, leaf1.key,
            "Key should match for minimal index case"
        );
        assert_eq!(
            stored_leaf1.value, leaf1.value,
            "Value should match for minimal index case"
        );

        let large_index = (1 << (ACCOUNT_TREE_HEIGHT - 1)) - 1;
        let leaf2 = IndexedMerkleLeaf {
            next_index: large_index + 1,
            key: U256::from(large_index as u32),
            next_key: U256::from((large_index + 1) as u32),
            value: 9999,
        };
        tree.update_leaf(&mut tx, timestamp, large_index, leaf2.clone())
            .await?;
        let stored_leaf2 = tree.get_leaf(&mut tx, timestamp, large_index).await?;
        assert_eq!(
            stored_leaf2.key, leaf2.key,
            "Key should match for large index case"
        );
        assert_eq!(
            stored_leaf2.value, leaf2.value,
            "Value should match for large index case"
        );

        let leaf3 = IndexedMerkleLeaf {
            next_index: 2,
            key: U256::from(1),
            next_key: U256::from(2),
            value: 200,
        };
        tree.update_leaf(&mut tx, timestamp, 1, leaf3.clone())
            .await?;
        let stored_leaf3 = tree.get_leaf(&mut tx, timestamp, 1).await?;
        assert_eq!(
            stored_leaf3.value, leaf3.value,
            "Value should be updated for same index update"
        );

        let new_timestamp = 789;
        let leaf4 = IndexedMerkleLeaf {
            next_index: 2,
            key: U256::from(1),
            next_key: U256::from(2),
            value: 300,
        };
        tree.update_leaf(&mut tx, new_timestamp, 1, leaf4.clone())
            .await?;

        let stored_leaf_old_ts = tree.get_leaf(&mut tx, timestamp, 1).await?;
        assert_eq!(
            stored_leaf_old_ts.value, leaf3.value,
            "Old timestamp should still see the old value"
        );
        let stored_leaf_new_ts = tree.get_leaf(&mut tx, new_timestamp, 1).await?;
        assert_eq!(
            stored_leaf_new_ts.value, leaf4.value,
            "New timestamp should see the updated value"
        );

        let root = tree
            .sql_node_hashes
            .get_root(&mut tx, new_timestamp)
            .await?;
        let proof = tree.prove_inclusion(&mut tx, new_timestamp, 1).await?;
        let result = proof.verify(root, 1, leaf4.key);
        assert!(result, "Final proof verification failed");

        tx.rollback().await?;
        Ok(())
    }
}
