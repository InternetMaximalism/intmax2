use async_trait::async_trait;
use error::MerkleTreeError;
use intmax2_zkp::{
    common::trees::account_tree::AccountMerkleProof, ethereum_types::u256::U256, utils::{
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
    }
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
pub trait IndexedMerkleTreeClient: std::fmt::Debug + Clone {
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
    use crate::trees::{
        merkle_tree::{
            sql_incremental_merkle_tree::SqlIncrementalMerkleTree, IncrementalMerkleTreeClient,
        },
        setup_test,
    };

    type V = u32;

    #[tokio::test]
    async fn test_speed_merkle_tree() -> anyhow::Result<()> {
        let height = 32;
        let n = 1 << 8;

        let database_url = setup_test();
        let tree = SqlIncrementalMerkleTree::<V>::new(&database_url, 0, height);
        tree.reset(0).await?;

        let timestamp = 0;
        let time = std::time::Instant::now();
        for i in 0..n {
            tree.push(timestamp, i as u32).await?;
        }
        println!(
            "SqlMerkleTree: {} leaves, {} height, {} seconds",
            n,
            height,
            time.elapsed().as_secs_f64()
        );

        Ok(())
    }
}
