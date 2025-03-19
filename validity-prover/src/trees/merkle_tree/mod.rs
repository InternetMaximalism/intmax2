use async_trait::async_trait;
use error::MerkleTreeError;
use intmax2_zkp::{
    common::trees::account_tree::AccountMerkleProof,
    ethereum_types::u256::U256,
    utils::{
        leafable::Leafable,
        leafable_hasher::LeafableHasher,
        poseidon_hash_out::PoseidonHashOut,
        trees::{
            incremental_merkle_tree::IncrementalMerkleProof,
            indexed_merkle_tree::{
                insertion::IndexedInsertionProof, leaf::IndexedMerkleLeaf,
                membership::MembershipProof, update::UpdateProof,
            },
        },
    },
};
use serde::{de::DeserializeOwned, Serialize};

pub mod error;
pub mod mock_incremental_merkle_tree;
pub mod mock_indexed_merkle_tree;
pub mod sql_incremental_merkle_tree;
pub mod sql_indexed_merkle_tree;
pub mod sql_node_hash;

pub type Hasher<V> = <V as Leafable>::LeafableHasher;
pub type HashOut<V> = <Hasher<V> as LeafableHasher>::HashOut;
pub type MTResult<T> = std::result::Result<T, MerkleTreeError>;

#[async_trait(?Send)]
pub trait IncrementalMerkleTreeClient<V: Leafable + Serialize + DeserializeOwned>:
    std::fmt::Debug + Clone
{
    fn height(&self) -> usize;
    async fn get_root(&self, timestamp: u64) -> MTResult<HashOut<V>>;
    async fn get_leaf(&self, timestamp: u64, position: u64) -> MTResult<V>;
    async fn len(&self, timestamp: u64) -> MTResult<usize>;
    async fn update_leaf(&self, timestamp: u64, position: u64, leaf: V) -> MTResult<()>;
    async fn push(&self, timestamp: u64, leaf: V) -> MTResult<()>;
    async fn prove(&self, timestamp: u64, position: u64) -> MTResult<IncrementalMerkleProof<V>>;
    async fn get_last_timestamp(&self) -> MTResult<u64>;
    async fn reset(&self, timestamp: u64) -> MTResult<()>;
}

#[async_trait(?Send)]
pub trait IndexedMerkleTreeClient: std::fmt::Debug {
    async fn get_root(&self, timestamp: u64) -> MTResult<PoseidonHashOut>;
    async fn get_leaf(&self, timestamp: u64, index: u64) -> MTResult<IndexedMerkleLeaf>;
    async fn len(&self, timestamp: u64) -> MTResult<usize>;
    async fn push(&self, timestamp: u64, leaf: IndexedMerkleLeaf) -> MTResult<()>;
    async fn get_last_timestamp(&self) -> MTResult<u64>;
    async fn reset(&self, timestamp: u64) -> MTResult<()>;

    async fn index(&self, timestamp: u64, key: U256) -> MTResult<Option<u64>>;
    async fn key(&self, timestamp: u64, index: u64) -> MTResult<U256>;

    async fn prove_inclusion(
        &self,
        timestamp: u64,
        account_id: u64,
    ) -> MTResult<AccountMerkleProof>;
    async fn prove_membership(&self, timestamp: u64, key: U256) -> MTResult<MembershipProof>;
    async fn insert(&self, timestamp: u64, key: U256, value: u64) -> MTResult<()>;
    async fn prove_and_insert(
        &self,
        timestamp: u64,
        key: U256,
        value: u64,
    ) -> MTResult<IndexedInsertionProof>;
    async fn prove_and_update(
        &self,
        timestamp: u64,
        key: U256,
        new_value: u64,
    ) -> MTResult<UpdateProof>;
}

#[cfg(test)]
mod tests {
    use intmax2_zkp::ethereum_types::u256::U256;
    use rand::Rng;

    use crate::trees::{
        merkle_tree::{
            sql_incremental_merkle_tree::SqlIncrementalMerkleTree,
            sql_indexed_merkle_tree::SqlIndexedMerkleTree, IncrementalMerkleTreeClient,
            IndexedMerkleTreeClient,
        },
        setup_test,
    };

    type V = u32;

    #[tokio::test]
    #[ignore]
    async fn test_speed_incremental_merkle_tree() -> anyhow::Result<()> {
        let height = 32;
        let n = 1 << 8;
        let mut rng = rand::thread_rng();

        let database_url = setup_test();
        let pool = sqlx::Pool::connect(&database_url).await?;

        let tree = SqlIncrementalMerkleTree::<V>::new(pool, rng.gen(), height);
        tree.reset(0).await?;

        let timestamp = 0;
        let time = std::time::Instant::now();
        for i in 0..n {
            tree.push(timestamp, i as u32).await?;
        }
        println!(
            "SqlIncrementMerkleTree: {} leaves, {} height, {} seconds",
            n,
            height,
            time.elapsed().as_secs_f64()
        );

        Ok(())
    }

    #[tokio::test]
    #[ignore]
    async fn test_speed_incremental_merkle_tree_with_select() -> anyhow::Result<()> {
        let height = 32;
        let n = 1000;
        let mut rng = rand::thread_rng();

        let database_url = setup_test();
        let pool = sqlx::Pool::connect(&database_url).await?;

        let tree = SqlIncrementalMerkleTree::<V>::new(pool, rng.gen(), height);
        tree.reset(0).await?;

        let time = std::time::Instant::now();
        for i in 0..n {
            let timestamp = i;
            tree.push(timestamp, i as u32).await?;
        }
        println!(
            "SqlIncrementMerkleTree.push: {} leaves, {} height, {} seconds",
            n,
            height,
            time.elapsed().as_secs_f64()
        );

        let n = n * 5;

        // SELECT - pattern 1
        let time = std::time::Instant::now();
        for _i in 0..n {
            tree.get_last_timestamp().await?;
        }
        println!(
            "SqlIncrementMerkleTree.get_last_timestamp: {} times, {} seconds",
            n,
            time.elapsed().as_secs_f64()
        );

        // SELECT - pattern 2
        let time = std::time::Instant::now();
        for _i in 0..n {
            tree.get_leaf(n * 2, 1).await?;
            tree.get_leaf(n * 2, n / 4).await?;
            tree.get_leaf(n * 2, n / 2).await?;
        }
        println!(
            "SqlIncrementMerkleTree.get_leaf: {} times, {} seconds",
            n,
            time.elapsed().as_secs_f64()
        );

        Ok(())
    }

    #[tokio::test]
    #[ignore]
    async fn test_speed_incremental_merkle_tree_prove() -> anyhow::Result<()> {
        let height = 32;
        let n = 5000;
        let mut rng = rand::thread_rng();

        let database_url = setup_test();
        let pool = sqlx::Pool::connect(&database_url).await?;

        let tree = SqlIncrementalMerkleTree::<V>::new(pool, rng.gen(), height);
        tree.reset(0).await?;

        let time = std::time::Instant::now();
        for i in 0..n {
            let timestamp = i;
            tree.prove(timestamp, i).await?;
        }
        println!(
            "SqlIncrementMerkleTree.prove: {} times, {} height, {} seconds",
            n,
            height,
            time.elapsed().as_secs_f64()
        );

        Ok(())
    }

    #[tokio::test]
    #[ignore]
    async fn test_speed_incremental_merkle_tree_reset() -> anyhow::Result<()> {
        let height = 32;
        let n = 1000;
        let mut rng = rand::thread_rng();

        let database_url = setup_test();
        let pool = sqlx::Pool::connect(&database_url).await?;

        let tree = SqlIncrementalMerkleTree::<V>::new(pool, rng.gen(), height);
        tree.reset(0).await?;

        for h in 0..5 {
            for i in 0..n {
                let timestamp = i;
                tree.push(timestamp, i as u32).await?;
            }
            let time = std::time::Instant::now();
            tree.reset(n / 2).await?;
            tree.reset(n / 4).await?;
            tree.reset(0).await?;
            tree.reset(0).await?;
            println!(
                "SqlIncrementMerkleTree.reset.loop{}: {} leaves, {} height, {} seconds",
                h,
                n,
                height,
                time.elapsed().as_secs_f64()
            );
        }

        Ok(())
    }

    #[tokio::test]
    #[ignore]
    async fn test_speed_indexed_merkle_tree() -> anyhow::Result<()> {
        let height = 32;
        let n = 1 << 10;
        let mut rng = rand::thread_rng();

        let database_url = setup_test();
        let pool = sqlx::Pool::connect(&database_url).await?;

        let tree = SqlIndexedMerkleTree::new(pool, rng.gen(), height);
        tree.reset(0).await?;
        tree.initialize().await?;

        for timestamp in 0..n {
            for i in 0..n {
                let key = U256::rand(&mut rng);
                let _ = tree.prove_and_insert(timestamp, key, i).await?;
            }
            print_time(&tree, timestamp).await?;
        }

        async fn print_time(tree: &SqlIndexedMerkleTree, timestamp: u64) -> anyhow::Result<()> {
            let now = std::time::Instant::now();
            let mut rng = rand::thread_rng();
            let _ = tree
                .prove_and_insert(timestamp, U256::rand(&mut rng), 0)
                .await?;
            let len = tree.len(timestamp).await?;
            println!("leaf.len: {}, insert time: {:?}", len, now.elapsed());
            Ok(())
        }

        Ok(())
    }
}
