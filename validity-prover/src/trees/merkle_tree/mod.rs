use async_trait::async_trait;
use error::MerkleTreeError;
use intmax2_zkp::{
    ethereum_types::u256::U256,
    utils::{
        leafable::Leafable,
        leafable_hasher::LeafableHasher,
        poseidon_hash_out::PoseidonHashOut,
        trees::{
            indexed_merkle_tree::{leaf::IndexedMerkleLeaf, IndexedMerkleProof},
            merkle_tree::MerkleProof,
        },
    },
};
use serde::{de::DeserializeOwned, Serialize};

pub mod error;
// pub mod mock_indexed_merkle_tree;
pub mod mock_merkle_tree;
pub mod sql_indexed_merkle_tree;
pub mod sql_merkle_tree;
pub mod sql_node_hash;

pub type Hasher<V> = <V as Leafable>::LeafableHasher;
pub type HashOut<V> = <Hasher<V> as LeafableHasher>::HashOut;
pub type MTResult<T> = std::result::Result<T, MerkleTreeError>;

#[async_trait(?Send)]
pub trait MerkleTreeClient<V: Leafable + Serialize + DeserializeOwned>:
    std::fmt::Debug + Clone
{
    fn height(&self) -> usize;
    async fn get_root(&self, timestamp: u64) -> MTResult<HashOut<V>>;
    async fn get_leaf(&self, timestamp: u64, position: u64) -> MTResult<V>;
    async fn len(&self, timestamp: u64) -> MTResult<usize>;
    async fn push(&self, timestamp: u64, leaf: V) -> MTResult<()>;
    async fn prove(&self, timestamp: u64, position: u64) -> MTResult<MerkleProof<V>>;
    async fn get_last_timestamp(&self) -> MTResult<u64>;
    async fn reset(&self, timestamp: u64) -> MTResult<()>;
}

#[async_trait(?Send)]
pub trait IndexedMerkleTreeClient: std::fmt::Debug + Clone {
    async fn get_root(&self, timestamp: u64) -> MTResult<PoseidonHashOut>;
    async fn get_leaf(&self, timestamp: u64, index: u64) -> MTResult<IndexedMerkleLeaf>;
    async fn len(&self, timestamp: u64) -> MTResult<usize>;
    async fn prove(&self, timestamp: u64, index: u64) -> MTResult<IndexedMerkleProof>;
    async fn update(&self, timestamp: u64, key: U256, value: u64) -> MTResult<()>;
    async fn reset(&self, timestamp: u64) -> MTResult<()>;
}

// #[cfg(test)]
// mod tests {
//     use crate::trees::setup_test;

//     use super::sql_merkle_tree::SqlMerkleTree;
//     use crate::trees::merkle_tree::MerkleTreeClient;

//     type V = u32;

//     #[tokio::test]
//     async fn test_speed_merkle_tree() -> anyhow::Result<()> {
//         let height = 32;
//         let n = 1 << 12;

//         let database_url = setup_test();
//         let tree = SqlMerkleTree::<V>::new(&database_url, 0, height);
//         tree.reset(0).await?;

//         let timestamp = 0;
//         let time = std::time::Instant::now();
//         for i in 0..n {
//             tree.push(timestamp, i as u32).await?;
//         }
//         println!(
//             "SqlMerkleTree: {} leaves, {} height, {} seconds",
//             n,
//             height,
//             time.elapsed().as_secs_f64()
//         );

//         Ok(())
//     }
// }
