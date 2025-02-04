use intmax2_zkp::utils::{
    leafable::Leafable, trees::incremental_merkle_tree::IncrementalMerkleProof,
};
use serde::{de::DeserializeOwned, Serialize};

use crate::trees::merkle_tree::{HashOut, MTResult, MerkleTreeClient};

#[derive(Debug, Clone)]
pub struct HistoricalIncrementalMerkleTree<
    V: Leafable + Serialize + DeserializeOwned,
    DB: MerkleTreeClient<V>,
> {
    merkle_tree: DB,
    _phantom: std::marker::PhantomData<V>,
}

impl<V: Leafable + Serialize + DeserializeOwned, DB: MerkleTreeClient<V>>
    HistoricalIncrementalMerkleTree<V, DB>
{
    pub fn new(merkle_tree: DB) -> Self {
        HistoricalIncrementalMerkleTree {
            merkle_tree,
            _phantom: std::marker::PhantomData,
        }
    }

    pub fn height(&self) -> usize {
        self.merkle_tree.height()
    }

    pub async fn len(&self, timestamp: u64) -> MTResult<usize> {
        let len = self.merkle_tree.get_num_leaves(timestamp).await?;
        Ok(len)
    }

    pub async fn update(&self, timestamp: u64, index: u64, leaf: V) -> MTResult<()> {
        self.merkle_tree.update_leaf(timestamp, index, leaf).await?;
        Ok(())
    }

    pub async fn push(&self, timestamp: u64, leaf: V) -> MTResult<()> {
        let index = self.len(timestamp).await? as u64;
        self.merkle_tree.update_leaf(timestamp, index, leaf).await?;
        Ok(())
    }

    pub async fn get_root(&self, timestamp: u64) -> MTResult<HashOut<V>> {
        let root = self.merkle_tree.get_root(timestamp).await?;
        Ok(root)
    }

    pub async fn get_leaves(&self, timestamp: u64) -> MTResult<Vec<V>> {
        let leaves = self.merkle_tree.get_leaves(timestamp).await?;
        Ok(leaves)
    }

    pub async fn get_leaf(&self, timestamp: u64, index: u64) -> MTResult<V> {
        let leaf = self.merkle_tree.get_leaf(timestamp, index).await?;
        Ok(leaf)
    }

    pub async fn prove(&self, timestamp: u64, index: u64) -> MTResult<IncrementalMerkleProof<V>> {
        let proof = self.merkle_tree.prove(timestamp, index).await?;
        Ok(IncrementalMerkleProof(proof))
    }

    pub async fn get_last_timestamp(&self) -> MTResult<u64> {
        let timestamp = self.merkle_tree.get_last_timestamp().await?;
        Ok(timestamp)
    }
}

#[cfg(test)]
mod tests {
    use intmax2_zkp::utils::trees::incremental_merkle_tree::IncrementalMerkleTree;

    use crate::trees::{
        incremental_merkle_tree::HistoricalIncrementalMerkleTree,
        merkle_tree::{
            mock_incremental_merkle_tree::MockIncrementalMerkleTree,
            sql_incremental_merkle_tree::SqlIncrementalMerkleTree, MerkleTreeClient,
        },
        utils::bit_path::BitPath,
    };

    #[tokio::test]
    async fn merkle_tree_nodes() -> anyhow::Result<()> {
        let height = 3;
        type V = u32;

        let db = MockIncrementalMerkleTree::<V>::new(height);
        let db_tree = HistoricalIncrementalMerkleTree::new(db);
        let timestamp = db_tree.get_last_timestamp().await?;
        for i in 0..4 {
            db_tree.push(timestamp, i as u32).await?;
        }

        for i in 0..4 {
            let bit_path = BitPath::new(height as u32, i);
            let node = db_tree
                .merkle_tree
                .get_node_hash(timestamp, bit_path)
                .await?;
            println!("bit_path: {:?}, node: {:?}", bit_path, node);
        }

        let bit_path = BitPath::new(height as u32, 1);
        println!("bit_path: {:?}", bit_path);
        println!(
            "node1: {:?}",
            db_tree.merkle_tree.hash_nodes.read().await.get(&bit_path)
        );
        Ok(())
    }

    #[tokio::test]
    async fn merkle_tree_with_leaves() -> anyhow::Result<()> {
        let height = 2;
        let database_url = crate::trees::setup_test();

        let tag = 1;

        type V = u32;

        let db = SqlIncrementalMerkleTree::<V>::new(&database_url, tag, height);
        db.reset().await?;
        let db = MockIncrementalMerkleTree::<V>::new(height);
        let db_tree = HistoricalIncrementalMerkleTree::new(db);

        let timestamp = db_tree.get_last_timestamp().await?;
        db_tree.push(timestamp, 1).await?;
        db_tree.push(timestamp, 2).await?;
        dbg!(&db_tree.merkle_tree.hash_nodes);

        let root_db = db_tree.get_root(timestamp).await?;

        let mut tree = IncrementalMerkleTree::<V>::new(height);
        tree.push(1);
        tree.push(2);
        let root = tree.get_root();
        assert_eq!(root_db, root);
        Ok(())
    }
}
