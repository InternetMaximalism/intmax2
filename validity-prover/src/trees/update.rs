use anyhow::ensure;
use hashbrown::HashMap;
use intmax2_zkp::{
    common::{
        trees::{account_tree::AccountRegistrationProof, sender_tree::get_sender_leaves},
        witness::{
            block_witness::BlockWitness, full_block::FullBlock,
            validity_transition_witness::ValidityTransitionWitness,
            validity_witness::ValidityWitness,
        },
    },
    constants::{ACCOUNT_TREE_HEIGHT, NUM_SENDERS_IN_BLOCK},
    ethereum_types::{account_id::AccountIdPacked, bytes32::Bytes32, u256::U256},
    utils::trees::indexed_merkle_tree::membership::MembershipProof,
};

use crate::trees::merkle_tree::IncrementalMerkleTreeClient;

use super::merkle_tree::IndexedMerkleTreeClient;

pub async fn to_block_witness<
    HistoricalAccountTree: IndexedMerkleTreeClient,
    HistoricalBlockHashTree: IncrementalMerkleTreeClient<Bytes32>,
>(
    full_block: &FullBlock,
    timestamp: u64,
    account_tree: &HistoricalAccountTree,
    block_tree: &HistoricalBlockHashTree,
) -> anyhow::Result<BlockWitness> {
    ensure!(
        full_block.block.block_number != 0,
        "genesis block is not allowed"
    );
    let is_registration_block = full_block
        .signature
        .block_sign_payload
        .is_registration_block;
    let (pubkeys, account_id_packed, account_merkle_proofs, account_membership_proofs) =
        if is_registration_block {
            let mut pubkeys = full_block.pubkeys.clone().ok_or(anyhow::anyhow!(
                "pubkeys is not given while it is registration block"
            ))?;
            pubkeys.resize(NUM_SENDERS_IN_BLOCK, U256::dummy_pubkey());
            let mut account_membership_proofs = Vec::new();
            let mut cached_proofs: HashMap<U256, MembershipProof> = HashMap::new();
            for pubkey in pubkeys.iter() {
                if cached_proofs.contains_key(pubkey) {
                    account_membership_proofs.push(cached_proofs[pubkey].clone());
                    continue;
                }
                let is_dummy = pubkey.is_dummy_pubkey();
                if !(account_tree.index(timestamp, *pubkey).await?.is_none() || is_dummy) {
                    log::warn!(
                        "Invalid block {}: account already exists",
                        full_block.block.block_number
                    );
                }
                let proof = account_tree.prove_membership(timestamp, *pubkey).await?;
                account_membership_proofs.push(proof.clone());
                cached_proofs.insert(*pubkey, proof);
            }
            (pubkeys, None, None, Some(account_membership_proofs))
        } else {
            let account_id_trimmed_bytes = full_block.account_ids.clone().ok_or(
                anyhow::anyhow!("account_ids is not given while it is non-registration block"),
            )?;
            let account_id_packed = AccountIdPacked::from_trimmed_bytes(&account_id_trimmed_bytes)
                .map_err(|e| anyhow::anyhow!("error while recovering packed account ids {}", e))?;
            let account_ids = account_id_packed.unpack();
            let mut account_merkle_proofs = Vec::new();
            let mut pubkeys = Vec::new();
            for account_id in account_ids {
                let pubkey = account_tree.key(timestamp, account_id.0).await?;
                let proof = account_tree
                    .prove_inclusion(timestamp, account_id.0)
                    .await?;
                pubkeys.push(pubkey);
                account_merkle_proofs.push(proof);
            }
            (
                pubkeys,
                Some(account_id_packed),
                Some(account_merkle_proofs),
                None,
            )
        };
    let prev_account_tree_root = account_tree.get_root(timestamp).await?;
    let prev_next_account_id = account_tree.len(timestamp).await? as u64;
    let prev_block_tree_root = block_tree.get_root(timestamp).await?;
    let block_witness = BlockWitness {
        block: full_block.block.clone(),
        signature: full_block.signature.clone(),
        pubkeys: pubkeys.clone(),
        prev_account_tree_root,
        prev_next_account_id,
        prev_block_tree_root,
        account_id_packed,
        account_merkle_proofs,
        account_membership_proofs,
    };
    Ok(block_witness)
}

pub async fn update_trees<
    HistoricalAccountTree: IndexedMerkleTreeClient,
    HistoricalBlockHashTree: IncrementalMerkleTreeClient<Bytes32>,
>(
    block_witness: &BlockWitness,
    timestamp: u64,
    account_tree: &HistoricalAccountTree,
    block_tree: &HistoricalBlockHashTree,
) -> anyhow::Result<ValidityWitness> {
    let block_pis = block_witness.to_main_validation_pis().map_err(|e| {
        anyhow::anyhow!("failed to convert to main validation public inputs: {}", e)
    })?;
    let block_tree_len = block_tree.len(timestamp).await?;
    ensure!(
        block_pis.block_number == block_tree_len as u32,
        "block number mismatch"
    );

    // Update block tree
    let block_merkle_proof = block_tree
        .prove(timestamp, block_witness.block.block_number as u64)
        .await?;
    block_tree
        .push(timestamp, block_witness.block.hash())
        .await?;

    // Update account tree
    let sender_leaves =
        get_sender_leaves(&block_witness.pubkeys, block_witness.signature.sender_flag);
    let account_registration_proofs = {
        if block_pis.is_valid && block_pis.is_registration_block {
            let mut account_registration_proofs = Vec::new();
            for sender_leaf in &sender_leaves {
                let is_dummy_pubkey = sender_leaf.sender.is_dummy_pubkey();
                let will_update = sender_leaf.signature_included && !is_dummy_pubkey;
                let proof = if will_update {
                    account_tree
                        .prove_and_insert(
                            timestamp,
                            sender_leaf.sender,
                            block_pis.block_number as u64,
                        )
                        .await
                        .map_err(|e| {
                            anyhow::anyhow!("failed to prove and insert account_tree: {}", e)
                        })?
                } else {
                    AccountRegistrationProof::dummy(ACCOUNT_TREE_HEIGHT)
                };
                account_registration_proofs.push(proof);
            }
            Some(account_registration_proofs)
        } else {
            None
        }
    };

    let account_update_proofs = {
        if block_pis.is_valid && (!block_pis.is_registration_block) {
            let mut account_update_proofs = Vec::new();
            let block_number = block_pis.block_number;
            for sender_leaf in sender_leaves.iter() {
                let account_id = account_tree
                    .index(timestamp, sender_leaf.sender)
                    .await?
                    .unwrap();
                let prev_leaf = account_tree.get_leaf(timestamp, account_id).await?;
                let prev_last_block_number = prev_leaf.value as u32;
                let last_block_number = if sender_leaf.signature_included {
                    block_number
                } else {
                    prev_last_block_number
                };
                let proof = account_tree
                    .prove_and_update(timestamp, sender_leaf.sender, last_block_number as u64)
                    .await?;
                account_update_proofs.push(proof);
            }
            Some(account_update_proofs)
        } else {
            None
        }
    };

    let validity_transition_witness = ValidityTransitionWitness {
        sender_leaves,
        block_merkle_proof,
        account_registration_proofs,
        account_update_proofs,
    };
    Ok(ValidityWitness {
        validity_transition_witness,
        block_witness: block_witness.clone(),
    })
}
