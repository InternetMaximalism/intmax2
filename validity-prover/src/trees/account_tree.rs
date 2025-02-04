use crate::trees::{
    incremental_merkle_tree::HistoricalIncrementalMerkleTree,
    indexed_merkle_tree::HistoricalIndexedMerkleTree, merkle_tree::IncrementalMerkleTreeClient,
};
use anyhow::Result;
use intmax2_zkp::{
    common::trees::account_tree::AccountMerkleProof,
    ethereum_types::u256::U256,
    utils::trees::indexed_merkle_tree::{
        insertion::IndexedInsertionProof, leaf::IndexedMerkleLeaf, membership::MembershipProof,
        update::UpdateProof,
    },
};

type V = IndexedMerkleLeaf;
pub type HistoricalAccountTree<DB> = HistoricalIndexedMerkleTree<DB>;

impl<DB: IncrementalMerkleTreeClient<V>> HistoricalAccountTree<DB> {
    pub async fn initialize(db: DB) -> Result<Self> {
        let last_timestamp = db.get_last_timestamp().await?;
        let tree = HistoricalIndexedMerkleTree(HistoricalIncrementalMerkleTree::new(db));
        if last_timestamp == 0 {
            if tree.len(last_timestamp).await? == 0 {
                tree.0
                    .push(last_timestamp, IndexedMerkleLeaf::default())
                    .await?;
            }
            if tree.len(last_timestamp).await? == 1 {
                tree.insert(last_timestamp, U256::dummy_pubkey(), 0).await?; // add default account
            }
        }

        Ok(tree)
    }

    pub async fn prove_membership(&self, timestamp: u64, key: U256) -> Result<MembershipProof> {
        if let Some(index) = self.index(timestamp, key).await? {
            // inclusion proof
            Ok(MembershipProof {
                is_included: true,
                leaf_index: index,
                leaf: self.0.get_leaf(timestamp, index).await?,
                leaf_proof: self.0.prove(timestamp, index).await?,
            })
        } else {
            // exclusion proof
            let low_index = self.low_index(timestamp, key).await?; // unwrap is safe here
            Ok(MembershipProof {
                is_included: false,
                leaf_index: low_index,
                leaf: self.0.get_leaf(timestamp, low_index).await?,
                leaf_proof: self.0.prove(timestamp, low_index).await?,
            })
        }
    }

    pub async fn prove_inclusion(
        &self,
        timestamp: u64,
        account_id: u64,
    ) -> Result<AccountMerkleProof> {
        let leaf = self.get_leaf(timestamp, account_id).await?;
        let merkle_proof = self.prove(timestamp, account_id).await?;
        Ok(AccountMerkleProof { merkle_proof, leaf })
    }

    pub async fn insert(&self, timestamp: u64, key: U256, value: u64) -> Result<()> {
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
        self.0.update(timestamp, low_index, new_low_leaf).await?;
        self.0.push(timestamp, leaf).await?;
        Ok(())
    }

    pub async fn prove_and_insert(
        &self,
        timestamp: u64,
        key: U256,
        value: u64,
    ) -> Result<IndexedInsertionProof> {
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
        self.0.update(timestamp, low_index, new_low_leaf).await?;
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
    ) -> Result<UpdateProof> {
        let index = self
            .index(timestamp, key)
            .await?
            .ok_or_else(|| anyhow::anyhow!("key not found"))?;
        let prev_leaf = self.0.get_leaf(timestamp, index).await?;
        let new_leaf = IndexedMerkleLeaf {
            value: new_value,
            ..prev_leaf
        };
        self.0.update(timestamp, index, new_leaf).await?;
        Ok(UpdateProof {
            leaf_proof: self.0.prove(timestamp, index).await?,
            leaf_index: index,
            prev_leaf,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::trees::{
        account_tree::HistoricalAccountTree,
        merkle_tree::{sql_incremental_merkle_tree::SqlIncrementalMerkleTree, IncrementalMerkleTreeClient},
    };
    use intmax2_zkp::{
        common::trees::account_tree::AccountTree, constants::ACCOUNT_TREE_HEIGHT,
        utils::trees::indexed_merkle_tree::leaf::IndexedMerkleLeaf,
    };

    #[tokio::test]
    async fn test_account_tree() -> anyhow::Result<()> {
        let database_url = crate::trees::setup_test();

        let tag = 3;
        let db = SqlIncrementalMerkleTree::<IndexedMerkleLeaf>::new(
            &database_url,
            tag,
            ACCOUNT_TREE_HEIGHT,
        );
        db.reset().await?;
        let tree = HistoricalAccountTree::initialize(db).await?;

        let timestamp0 = 0;
        for i in 2..5 {
            tree.insert(timestamp0, i.into(), i.into()).await?;
        }
        let old_root = tree.get_root(timestamp0).await?;
        let old_leaves = tree.0.get_leaves(timestamp0).await?;

        let timestamp1 = 1;
        for i in 5..8 {
            tree.insert(timestamp1, i.into(), i.into()).await?;
        }
        let leaves = tree.0.get_leaves(timestamp0).await?;
        assert_eq!(leaves, old_leaves);

        let account_id = 3;
        let proof = tree.prove_inclusion(timestamp0, account_id).await?;
        let result = proof.verify(old_root, account_id, (account_id as u32).into());
        assert!(result);

        Ok(())
    }

    #[tokio::test]
    async fn test_comparison_account_tree() -> anyhow::Result<()> {
        let database_url = crate::trees::setup_test();
        let tag = 3;
        let db = SqlIncrementalMerkleTree::<IndexedMerkleLeaf>::new(
            &database_url,
            tag,
            ACCOUNT_TREE_HEIGHT,
        );
        db.reset(0).await?;
        let db_tree = HistoricalAccountTree::initialize(db).await?;
        let timestamp = db_tree.0.get_last_timestamp().await?;
        for i in 2..10 {
            db_tree.insert(timestamp, i.into(), i.into()).await?;
        }
        let db_root = db_tree.get_root(timestamp).await?;

        let mut tree = AccountTree::initialize();
        for i in 2..10 {
            tree.insert(i.into(), i.into())?;
        }
        let root = tree.get_root();
        assert_eq!(db_root, root);

        Ok(())
    }
}
