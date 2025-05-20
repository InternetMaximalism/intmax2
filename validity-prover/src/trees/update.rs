use super::merkle_tree::IndexedMerkleTreeClient;
use crate::trees::merkle_tree::IncrementalMerkleTreeClient;
use anyhow::ensure;
use futures::StreamExt as _;
use intmax2_zkp::{
    common::{
        trees::{
            account_tree::{AccountMerkleProof, AccountRegistrationProof},
            sender_tree::get_sender_leaves,
        },
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
use log::warn;
use std::{
    collections::{HashMap, HashSet},
    time::Instant,
};
use tracing::{info, instrument};

const PARALLELISM_LIMIT: usize = 10;

#[instrument(skip_all, fields(timestamp = timestamp))]
pub async fn to_block_witness<
    HistoricalAccountTree: IndexedMerkleTreeClient,
    HistoricalBlockHashTree: IncrementalMerkleTreeClient<Bytes32>,
>(
    full_block: &FullBlock,
    timestamp: u64,
    account_tree: &HistoricalAccountTree,
    block_tree: &HistoricalBlockHashTree,
) -> anyhow::Result<BlockWitness> {
    let instant = Instant::now();
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
            let (pubkeys, account_membership_proofs) =
                generate_account_membership_proofs(account_tree, full_block, timestamp).await?;
            (pubkeys, None, None, Some(account_membership_proofs))
        } else {
            let (pubkeys, account_id_packed, account_merkle_proofs) =
                generate_account_merkle_proofs(account_tree, full_block, timestamp).await?;
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
    info!(
        "block_witness generated : block_number:{}, took: {:?}",
        block_witness.block.block_number,
        instant.elapsed()
    );
    Ok(block_witness)
}

#[instrument(skip_all, fields(timestamp = timestamp))]
async fn generate_account_membership_proofs<HistoricalAccountTree: IndexedMerkleTreeClient>(
    account_tree: &HistoricalAccountTree,
    full_block: &FullBlock,
    timestamp: u64,
) -> anyhow::Result<(Vec<U256>, Vec<MembershipProof>)> {
    let mut pubkeys = full_block.pubkeys.clone().ok_or(anyhow::anyhow!(
        "pubkeys is not given while it is registration block"
    ))?;
    pubkeys.resize(NUM_SENDERS_IN_BLOCK, U256::dummy_pubkey());

    let futures = pubkeys
        .iter()
        .cloned()
        .collect::<HashSet<_>>()
        .into_iter()
        .map(|pubkey| async move {
            let proof = account_tree.prove_membership(timestamp, pubkey).await?;
            anyhow::Ok((pubkey, proof))
        });
    let results = futures::stream::iter(futures)
        .buffer_unordered(PARALLELISM_LIMIT)
        .collect::<Vec<_>>()
        .await;

    let mut proofs_map = HashMap::with_capacity(pubkeys.len());
    for result in results {
        let (pubkey, proof) =
            result.map_err(|e| anyhow::anyhow!("Failed to generate membership proof: {}", e))?;
        proofs_map.insert(pubkey, proof);
    }

    let account_membership_proofs = pubkeys
        .iter()
        .map(|pubkey| {
            proofs_map
                .get(pubkey)
                .ok_or_else(|| {
                    anyhow::anyhow!("Failed to generate membership proof for pubkey {}", pubkey)
                })
                .cloned()
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok((pubkeys, account_membership_proofs))
}

#[instrument(skip_all, fields(timestamp = timestamp))]
async fn generate_account_merkle_proofs<HistoricalAccountTree: IndexedMerkleTreeClient>(
    account_tree: &HistoricalAccountTree,
    full_block: &FullBlock,
    timestamp: u64,
) -> anyhow::Result<(Vec<U256>, AccountIdPacked, Vec<AccountMerkleProof>)> {
    let account_id_trimmed_bytes = full_block.account_ids.clone().ok_or(anyhow::anyhow!(
        "account_ids is not given while it is non-registration block"
    ))?;
    let account_id_packed = AccountIdPacked::from_trimmed_bytes(&account_id_trimmed_bytes)
        .map_err(|e| anyhow::anyhow!("error while recovering packed account ids {}", e))?;
    let account_ids = account_id_packed.unpack();

    let futures = account_ids
        .iter()
        .cloned()
        .collect::<HashSet<_>>()
        .into_iter()
        .map(|account_id| async move {
            let pubkey = account_tree.key(timestamp, account_id.0).await?;
            let proof = account_tree
                .prove_inclusion(timestamp, account_id.0)
                .await?;
            anyhow::Ok((account_id, pubkey, proof))
        });
    let results = futures::stream::iter(futures)
        .buffer_unordered(PARALLELISM_LIMIT)
        .collect::<Vec<_>>()
        .await;
    let mut proofs_map = HashMap::with_capacity(account_ids.len());
    for result in results {
        let (account_id, pubkey, proof) = result
            .map_err(|e| anyhow::anyhow!("Failed to generate account merkle proof: {}", e))?;
        proofs_map.insert(account_id, (pubkey, proof));
    }

    let mut pubkeys = Vec::with_capacity(account_ids.len());
    let mut account_merkle_proofs = Vec::with_capacity(account_ids.len());
    for account_id in account_ids {
        let (pubkey, proof) = proofs_map
            .get(&account_id)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Failed to generate account merkle proof for account id {}",
                    account_id.0
                )
            })?
            .clone();
        pubkeys.push(pubkey);
        account_merkle_proofs.push(proof);
    }

    Ok((pubkeys, account_id_packed, account_merkle_proofs))
}

#[instrument(skip_all, fields(timestamp = timestamp))]
pub async fn update_trees<
    HistoricalAccountTree: IndexedMerkleTreeClient,
    HistoricalBlockHashTree: IncrementalMerkleTreeClient<Bytes32>,
>(
    block_witness: &BlockWitness,
    timestamp: u64,
    account_tree: &HistoricalAccountTree,
    block_tree: &HistoricalBlockHashTree,
) -> anyhow::Result<ValidityWitness> {
    let instant = Instant::now();
    let block_pis = block_witness.to_main_validation_pis().map_err(|e| {
        anyhow::anyhow!("failed to convert to main validation public inputs: {}", e)
    })?;
    let block_tree_len = block_tree.len(timestamp).await?;
    ensure!(
        block_pis.block_number == block_tree_len as u32,
        "block number mismatch: witness {} != block tree len {}",
        block_pis.block_number,
        block_tree_len
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

    let validity_witness = ValidityWitness {
        validity_transition_witness,
        block_witness: block_witness.clone(),
    };

    let pis = validity_witness
        .to_validity_pis()
        .map_err(|e| anyhow::anyhow!("failed to convert to validity public inputs: {}", e))?;

    if !pis.is_valid_block && pis.tx_tree_root != Bytes32::default() {
        // tx_tree_root == 0x is empty block for deposit sync
        warn!(
            "block:{}, is_registration:{} is not valid",
            pis.public_state.block_number,
            block_witness
                .signature
                .block_sign_payload
                .is_registration_block
        );
    }

    info!(
        "validity_witness generated : block_number:{}, took: {:?}",
        block_witness.block.block_number,
        instant.elapsed()
    );

    Ok(validity_witness)
}
