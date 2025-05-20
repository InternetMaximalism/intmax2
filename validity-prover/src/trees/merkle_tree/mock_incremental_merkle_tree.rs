use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use intmax2_zkp::utils::{
    leafable::Leafable,
    leafable_hasher::LeafableHasher,
    trees::{
        bit_path::BitPath, incremental_merkle_tree::IncrementalMerkleProof,
        merkle_tree::MerkleProof,
    },
};
use serde::{de::DeserializeOwned, Serialize};
use tokio::sync::RwLock;

use super::{error::MerkleTreeError, HashOut, Hasher, IncrementalMerkleTreeClient, MTResult};

#[derive(Debug, Clone)]
pub struct HashNode<V: Leafable> {
    pub timestamp_value: u64,
    pub bit_path: BitPath,
    pub hash: HashOut<V>,
}

#[derive(Debug, Clone)]
pub struct Leaf<V: Leafable> {
    pub timestamp_value: u64,
    pub position: u64,
    pub leaf_hash: HashOut<V>,
    pub leaf: V,
}

#[derive(Debug, Clone)]
pub struct MockIncrementalMerkleTree<V: Leafable> {
    height: usize,
    pub zero_hashes: Vec<HashOut<V>>,
    pub hash_nodes: Arc<RwLock<HashMap<BitPath, Vec<HashNode<V>>>>>, // bit_path -> hash_nodes
    pub leaves: Arc<RwLock<HashMap<u64, Vec<Leaf<V>>>>>,             // position -> leaf
    pub leaves_len: Arc<RwLock<HashMap<u64, usize>>>,                // timestamp -> num_leaves
}

impl<V: Leafable + Serialize + DeserializeOwned> MockIncrementalMerkleTree<V> {
    pub fn new(height: usize) -> Self {
        let mut zero_hashes = vec![];
        let mut h = V::empty_leaf().hash();
        zero_hashes.push(h);
        for _ in 0..height {
            let new_h = Hasher::<V>::two_to_one(h, h);
            zero_hashes.push(new_h);
            h = new_h;
        }
        zero_hashes.reverse();
        MockIncrementalMerkleTree {
            height,
            zero_hashes,
            hash_nodes: Arc::new(RwLock::new(HashMap::new())),
            leaves: Arc::new(RwLock::new(HashMap::new())),
            leaves_len: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    async fn save_node(
        &self,
        timestamp: u64,
        bit_path: BitPath,
        hash: HashOut<V>,
    ) -> super::MTResult<()> {
        let node = HashNode {
            timestamp_value: timestamp,
            bit_path,
            hash,
        };
        let mut hash_nodes = self
            .hash_nodes
            .read()
            .await
            .get(&bit_path)
            .cloned()
            .unwrap_or_default();
        let conflicting_index = hash_nodes
            .iter()
            .enumerate()
            .find(|(_, hash_node)| {
                hash_node.timestamp_value == timestamp && hash_node.bit_path == bit_path
            })
            .map(|(i, _)| i);
        if conflicting_index.is_some() {
            // replace the conflicting node
            hash_nodes[conflicting_index.unwrap()] = node.clone();
        } else {
            hash_nodes.push(node.clone());
        }
        self.hash_nodes.write().await.insert(bit_path, hash_nodes);
        Ok(())
    }

    pub async fn get_node_hash(
        &self,
        timestamp: u64,
        bit_path: BitPath,
    ) -> super::MTResult<HashOut<V>> {
        let hash_nodes = self
            .hash_nodes
            .read()
            .await
            .get(&bit_path)
            .cloned()
            .unwrap_or_default();
        // Get the latest one that exists at that time
        let node_hash = hash_nodes
            .iter()
            .filter(|hash_node| hash_node.timestamp_value <= timestamp)
            .max_by_key(|hash_node| hash_node.timestamp_value)
            .map(|hash_node| hash_node.hash)
            .unwrap_or(self.zero_hashes[bit_path.len() as usize]);
        Ok(node_hash)
    }

    async fn save_leaf(&self, timestamp: u64, position: u64, leaf: V) -> super::MTResult<()> {
        let leaf = Leaf {
            timestamp_value: timestamp,
            position,
            leaf_hash: leaf.hash(),
            leaf,
        };
        let current_len = self.len(timestamp).await?;
        let next_len = ((position + 1) as usize).max(current_len);
        self.leaves
            .write()
            .await
            .entry(position)
            .or_insert_with(Vec::new)
            .push(leaf);
        self.leaves_len
            .write()
            .await
            .entry(timestamp)
            .insert_entry(next_len);
        Ok(())
    }

    pub async fn get_leaf(&self, timestamp: u64, position: u64) -> super::MTResult<V> {
        let leaves = self
            .leaves
            .read()
            .await
            .get(&position)
            .cloned()
            .unwrap_or_default();

        // Get the latest one that exists at that time
        let leaf = leaves
            .iter()
            .filter(|leaf| leaf.timestamp_value <= timestamp)
            .max_by_key(|leaf| leaf.timestamp_value)
            .map(|leaf| leaf.leaf.clone())
            .unwrap_or(V::empty_leaf());

        Ok(leaf)
    }

    pub async fn get_leaves(&self, timestamp: u64) -> super::MTResult<Vec<(u64, V)>> {
        let num_leaves = self.len(timestamp).await?;
        let mut leaves = vec![];
        for i in 0..num_leaves {
            let leaf = self.get_leaf(timestamp, i as u64).await?;
            leaves.push((i as u64, leaf));
        }
        leaves.sort_by_key(|(i, _)| *i);
        Ok(leaves)
    }

    pub async fn len(&self, timestamp: u64) -> super::MTResult<usize> {
        let leaves_lens: Vec<(u64, usize)> =
            self.leaves_len.read().await.clone().into_iter().collect();
        let (_ts, num_leaves) = leaves_lens
            .into_iter()
            .filter(|(ts, _)| *ts <= timestamp)
            .max_by_key(|(ts, _)| *ts)
            .unwrap_or((0, 0));
        Ok(num_leaves)
    }

    async fn get_sibling_hash(&self, timestamp: u64, path: BitPath) -> MTResult<HashOut<V>> {
        if path.is_empty() {
            return Err(MerkleTreeError::WrongPathLength(0));
        }
        self.get_node_hash(timestamp, path.sibling()).await
    }

    pub async fn update_leaf(&self, timestamp: u64, index: u64, leaf: V) -> super::MTResult<()> {
        let mut path = BitPath::new(self.height as u32, index);
        path.reverse();
        let mut h = leaf.hash();
        self.save_leaf(timestamp, index, leaf).await?;
        self.save_node(timestamp, path, h).await?;

        while !path.is_empty() {
            let sibling = self.get_sibling_hash(timestamp, path).await?;
            let b = path.pop().unwrap(); // safe to unwrap
            let new_h = if b {
                Hasher::<V>::two_to_one(sibling, h)
            } else {
                Hasher::<V>::two_to_one(h, sibling)
            };
            self.save_node(timestamp, path, new_h).await?;
            h = new_h;
        }
        Ok(())
    }

    pub async fn prove(
        &self,
        timestamp: u64,
        index: u64,
    ) -> super::MTResult<IncrementalMerkleProof<V>> {
        let mut path = BitPath::new(self.height as u32, index);
        path.reverse(); // path is big endian
        let mut siblings = vec![];
        while !path.is_empty() {
            siblings.push(self.get_sibling_hash(timestamp, path).await?);
            path.pop();
        }
        Ok(IncrementalMerkleProof(MerkleProof { siblings }))
    }

    pub async fn get_root(&self, timestamp: u64) -> MTResult<HashOut<V>> {
        self.get_node_hash(timestamp, BitPath::default()).await
    }

    pub async fn reset(&self, timestamp: u64) -> MTResult<()> {
        // delete everything that has a timestamp greater than or equal to the given timestamp
        self.hash_nodes.write().await.retain(|_, hash_nodes| {
            hash_nodes.retain(|hash_node| hash_node.timestamp_value < timestamp);
            !hash_nodes.is_empty()
        });
        self.leaves.write().await.retain(|_, leaves| {
            leaves.retain(|leaf| leaf.timestamp_value < timestamp);
            !leaves.is_empty()
        });
        self.leaves_len
            .write()
            .await
            .retain(|ts, _| *ts < timestamp);

        Ok(())
    }

    pub async fn get_last_timestamp(&self) -> u64 {
        let leaves = self.leaves.read().await.clone();
        let last_timestamp = leaves
            .values()
            .map(|leaves| {
                leaves
                    .iter()
                    .map(|leaf| leaf.timestamp_value)
                    .max()
                    .unwrap_or(0)
            })
            .max()
            .unwrap_or(0);
        last_timestamp
    }

    pub async fn push(&self, timestamp: u64, leaf: V) -> MTResult<()> {
        let index = self.len(timestamp).await? as u64;
        self.update_leaf(timestamp, index, leaf).await?;
        Ok(())
    }
}

#[async_trait(?Send)]
impl<V: Leafable + Serialize + DeserializeOwned> IncrementalMerkleTreeClient<V>
    for MockIncrementalMerkleTree<V>
{
    fn height(&self) -> usize {
        self.height
    }

    async fn get_root(&self, timestamp: u64) -> MTResult<HashOut<V>> {
        self.get_root(timestamp).await
    }

    async fn get_leaf(&self, timestamp: u64, position: u64) -> MTResult<V> {
        self.get_leaf(timestamp, position).await
    }

    async fn len(&self, timestamp: u64) -> MTResult<usize> {
        self.len(timestamp).await
    }

    async fn update_leaf(&self, timestamp: u64, position: u64, leaf: V) -> MTResult<()> {
        self.update_leaf(timestamp, position, leaf).await
    }

    async fn push(&self, timestamp: u64, leaf: V) -> MTResult<()> {
        self.push(timestamp, leaf).await
    }

    async fn prove(&self, timestamp: u64, position: u64) -> MTResult<IncrementalMerkleProof<V>> {
        self.prove(timestamp, position).await
    }

    async fn get_last_timestamp(&self) -> MTResult<u64> {
        let last_timestamp = self.get_last_timestamp().await;
        Ok(last_timestamp)
    }

    async fn reset(&self, timestamp: u64) -> MTResult<()> {
        self.reset(timestamp).await
    }
}
