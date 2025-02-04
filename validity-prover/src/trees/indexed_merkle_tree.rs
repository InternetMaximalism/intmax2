use intmax2_zkp::utils::{
    poseidon_hash_out::PoseidonHashOut,
    trees::indexed_merkle_tree::{leaf::IndexedMerkleLeaf, IndexedMerkleProof},
};

use crate::trees::{
    incremental_merkle_tree::HistoricalIncrementalMerkleTree,
    merkle_tree::IncrementalMerkleTreeClient,
};
use anyhow::Result;

type V = IndexedMerkleLeaf;

#[derive(Debug, Clone)]
pub struct HistoricalIndexedMerkleTree<DB: IncrementalMerkleTreeClient<V>>(
    pub HistoricalIncrementalMerkleTree<V, DB>,
);

impl<DB: IncrementalMerkleTreeClient<V>> HistoricalIndexedMerkleTree<DB> {
    pub async fn get_root(&self, timestamp: u64) -> Result<PoseidonHashOut> {
        let root = self.0.get_root(timestamp).await?;
        Ok(root)
    }

    pub async fn get_leaf(&self, timestamp: u64, index: u64) -> Result<IndexedMerkleLeaf> {
        let leaf = self.0.get_leaf(timestamp, index).await?;
        Ok(leaf)
    }

    pub async fn prove(&self, timestamp: u64, index: u64) -> Result<IndexedMerkleProof> {
        let proof = self.0.prove(timestamp, index).await?;
        Ok(proof)
    }

    pub async fn len(&self, timestamp: u64) -> Result<usize> {
        let len = self.0.len(timestamp).await?;
        Ok(len)
    }
}
