use intmax2_zkp::{
    common::trees::account_tree::AccountMerkleProof,
    ethereum_types::u256::U256,
    utils::{
        poseidon_hash_out::PoseidonHashOut,
        trees::indexed_merkle_tree::{
            insertion::IndexedInsertionProof, leaf::IndexedMerkleLeaf, membership::MembershipProof,
            update::UpdateProof, IndexedMerkleProof,
        },
    },
};

use crate::trees::merkle_tree::error::MerkleTreeError;

use super::{
    mock_incremental_merkle_tree::MockIncrementalMerkleTree, IndexedMerkleTreeClient, MTResult,
};

type V = IndexedMerkleLeaf;

#[derive(Debug, Clone)]
pub struct MockIndexedMerkleTree(MockIncrementalMerkleTree<V>);

impl MockIndexedMerkleTree {
    pub async fn get_root(&self, timestamp: u64) -> MTResult<PoseidonHashOut> {
        let root = self.0.get_root(timestamp).await?;
        Ok(root)
    }

    pub async fn get_leaf(&self, timestamp: u64, index: u64) -> MTResult<IndexedMerkleLeaf> {
        let leaf = self.0.get_leaf(timestamp, index).await?;
        Ok(leaf)
    }

    pub async fn prove(&self, timestamp: u64, index: u64) -> MTResult<IndexedMerkleProof> {
        let proof = self.0.prove(timestamp, index).await?;
        Ok(proof)
    }

    pub async fn low_index(&self, timestamp: u64, key: U256) -> MTResult<u64> {
        let low_leaf_candidates = self
            .0
            .get_leaves(timestamp)
            .await?
            .into_iter()
            .filter(|(_, leaf)| {
                (leaf.key < key) && (key < leaf.next_key || leaf.next_key == U256::default())
            })
            .collect::<Vec<_>>();
        if low_leaf_candidates.is_empty() {
            return Err(MerkleTreeError::InternalError(
                "key already exists".to_string(),
            ));
        }
        if low_leaf_candidates.len() > 1 {
            return Err(MerkleTreeError::InternalError(
                "low_index: too many candidates".to_string(),
            ));
        }
        let (low_leaf_index, _) = low_leaf_candidates[0];
        Ok(low_leaf_index)
    }

    pub async fn index(&self, timestamp: u64, key: U256) -> MTResult<Option<u64>> {
        let leaf_candidates = self
            .0
            .get_leaves(timestamp)
            .await?
            .into_iter()
            .filter(|(_, leaf)| leaf.key == key)
            .collect::<Vec<_>>();
        if leaf_candidates.is_empty() {
            return Ok(None);
        }
        assert!(
            leaf_candidates.len() == 1,
            "find_index: too many candidates"
        );
        let (leaf_index, _) = leaf_candidates[0];
        Ok(Some(leaf_index))
    }

    pub async fn key(&self, timestamp: u64, index: u64) -> MTResult<U256> {
        let key = self.0.get_leaf(timestamp, index).await?.key;
        Ok(key)
    }

    pub async fn update(&self, timestamp: u64, key: U256, value: u64) -> MTResult<()> {
        let index = self.index(timestamp, key).await?.ok_or_else(|| {
            MerkleTreeError::InternalError("Error: key doesn't exist".to_string())
        })?;
        let mut leaf = self.0.get_leaf(timestamp, index).await?;
        leaf.value = value;
        self.0.update_leaf(timestamp, index, leaf).await?;
        Ok(())
    }

    pub async fn len(&self, timestamp: u64) -> MTResult<usize> {
        let len = self.0.len(timestamp).await?;
        Ok(len)
    }

    pub async fn get_last_timestamp(&self) -> u64 {
        self.0.get_last_timestamp().await
    }

    pub async fn reset(&self, timestamp: u64) -> MTResult<()> {
        self.0.reset(timestamp).await?;
        Ok(())
    }

    pub async fn prove_membership(&self, timestamp: u64, key: U256) -> MTResult<MembershipProof> {
        if let Some(index) = self.index(timestamp, key).await? {
            // inclusion proof
            Ok(MembershipProof {
                is_included: true,
                leaf_index: index,
                leaf: self.0.get_leaf(timestamp, index).await?,
                leaf_proof: self.prove(timestamp, index).await?,
            })
        } else {
            // exclusion proof
            let low_index = self.low_index(timestamp, key).await?; // unwrap is safe here
            Ok(MembershipProof {
                is_included: false,
                leaf_index: low_index,
                leaf: self.0.get_leaf(timestamp, low_index).await?,
                leaf_proof: self.prove(timestamp, low_index).await?,
            })
        }
    }

    pub async fn prove_inclusion(
        &self,
        timestamp: u64,
        account_id: u64,
    ) -> MTResult<AccountMerkleProof> {
        let leaf = self.get_leaf(timestamp, account_id).await?;
        let merkle_proof = self.prove(timestamp, account_id).await?;
        Ok(AccountMerkleProof { merkle_proof, leaf })
    }

    pub async fn insert(&self, timestamp: u64, key: U256, value: u64) -> MTResult<()> {
        let index = self.0.len(timestamp).await? as u64;
        let low_index = self.low_index(timestamp, key).await?;
        let prev_low_leaf = self.0.get_leaf(timestamp, low_index).await?;
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
        self.0
            .update_leaf(timestamp, low_index, new_low_leaf)
            .await?;
        self.0.push(timestamp, leaf).await?;
        Ok(())
    }

    pub async fn prove_and_insert(
        &self,
        timestamp: u64,
        key: U256,
        value: u64,
    ) -> MTResult<IndexedInsertionProof> {
        let index = self.0.len(timestamp).await? as u64;
        let low_index = self.low_index(timestamp, key).await?;
        let prev_low_leaf = self.0.get_leaf(timestamp, low_index).await?;
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
        let low_leaf_proof = self.0.prove(timestamp, low_index).await?;
        self.0
            .update_leaf(timestamp, low_index, new_low_leaf)
            .await?;
        self.0.push(timestamp, leaf).await?;
        let leaf_proof = self.0.prove(timestamp, index).await?;
        Ok(IndexedInsertionProof {
            index,
            low_leaf_proof,
            leaf_proof,
            low_leaf_index: low_index,
            prev_low_leaf,
        })
    }

    pub async fn prove_and_update(
        &self,
        timestamp: u64,
        key: U256,
        new_value: u64,
    ) -> MTResult<UpdateProof> {
        let index = self.index(timestamp, key).await?.ok_or_else(|| {
            MerkleTreeError::InternalError("Error: key doesn't exist".to_string())
        })?;
        let prev_leaf = self.0.get_leaf(timestamp, index).await?;
        let new_leaf = IndexedMerkleLeaf {
            value: new_value,
            ..prev_leaf
        };
        self.0.update_leaf(timestamp, index, new_leaf).await?;
        Ok(UpdateProof {
            leaf_proof: self.0.prove(timestamp, index).await?,
            leaf_index: index,
            prev_leaf,
        })
    }
}

#[async_trait::async_trait(?Send)]
impl IndexedMerkleTreeClient for MockIndexedMerkleTree {
    async fn get_root(&self, timestamp: u64) -> MTResult<PoseidonHashOut> {
        self.get_root(timestamp).await
    }

    async fn get_leaf(&self, timestamp: u64, index: u64) -> MTResult<IndexedMerkleLeaf> {
        self.get_leaf(timestamp, index).await
    }

    async fn len(&self, timestamp: u64) -> MTResult<usize> {
        self.len(timestamp).await
    }

    async fn push(&self, timestamp: u64, leaf: IndexedMerkleLeaf) -> MTResult<()> {
        self.insert(timestamp, leaf.key, leaf.value).await
    }

    async fn get_last_timestamp(&self) -> MTResult<u64> {
        Ok(self.get_last_timestamp().await)
    }

    async fn reset(&self, timestamp: u64) -> MTResult<()> {
        self.reset(timestamp).await
    }

    async fn index(&self, timestamp: u64, key: U256) -> MTResult<Option<u64>> {
        self.index(timestamp, key).await
    }

    async fn key(&self, timestamp: u64, index: u64) -> MTResult<U256> {
        self.key(timestamp, index).await
    }

    async fn prove_inclusion(
        &self,
        timestamp: u64,
        account_id: u64,
    ) -> MTResult<AccountMerkleProof> {
        self.prove_inclusion(timestamp, account_id).await
    }

    async fn prove_membership(&self, timestamp: u64, key: U256) -> MTResult<MembershipProof> {
        self.prove_membership(timestamp, key).await
    }

    async fn insert(&self, timestamp: u64, key: U256, value: u64) -> MTResult<()> {
        self.insert(timestamp, key, value).await
    }

    async fn prove_and_insert(
        &self,
        timestamp: u64,
        key: U256,
        value: u64,
    ) -> MTResult<IndexedInsertionProof> {
        self.prove_and_insert(timestamp, key, value).await
    }

    async fn prove_and_update(
        &self,
        timestamp: u64,
        key: U256,
        new_value: u64,
    ) -> MTResult<UpdateProof> {
        self.prove_and_update(timestamp, key, new_value).await
    }
}
