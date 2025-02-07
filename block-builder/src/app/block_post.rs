use ethers::types::H256;
use intmax2_client_sdk::external_api::{
    contract::rollup_contract::RollupContract, validity_prover::ValidityProverClient,
};
use intmax2_interfaces::api::validity_prover::interface::ValidityProverClientInterface;
use intmax2_zkp::{
    common::block_builder::{construct_signature, SenderWithSignature, UserSignature},
    ethereum_types::{account_id_packed::AccountIdPacked, bytes32::Bytes32, u256::U256},
};
use serde::{Deserialize, Serialize};

use super::error::BlockBuilderError;

const PENALTY_FEE_POLLING_INTERVAL: u64 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockPost {
    pub is_registration_block: bool,
    pub tx_tree_root: Bytes32,
    pub expiry: u64,
    pub pubkeys: Vec<U256>, // sorted & padded pubkeys
    pub pubkey_hash: Bytes32,
    pub signatures: Vec<UserSignature>,
}

pub async fn post_block(
    block_builder_private_key: H256,
    eth_allowance_for_block: U256,
    rollup_contract: &RollupContract,
    validity_prover_client: &ValidityProverClient,
    block_post: BlockPost,
) -> Result<(), BlockBuilderError> {
    // wait until penalty fee is below allowance
    loop {
        let penalty_fee = rollup_contract.get_penalty().await?;
        if penalty_fee <= eth_allowance_for_block {
            break;
        }
        log::warn!(
            "Penalty fee is above allowance: penalty_fee={}, allowance={}",
            penalty_fee,
            eth_allowance_for_block
        );
        tokio::time::sleep(tokio::time::Duration::from_secs(
            PENALTY_FEE_POLLING_INTERVAL,
        ))
        .await;
    }

    // construct signature
    let mut account_id_packed = None;
    let mut eliminated_pubkeys = Vec::new();
    if block_post.is_registration_block {
        for pubkey in block_post.pubkeys.iter() {
            if pubkey.is_dummy_pubkey() {
                // ignore dummy pubkey
                continue;
            }
            let account_info = validity_prover_client.get_account_info(*pubkey).await?;
            if account_info.account_id.is_some() {
                eliminated_pubkeys.push(*pubkey);
            }
        }
    } else {
        let mut account_ids = Vec::new();
        for pubkey in block_post.pubkeys.iter() {
            if pubkey.is_dummy_pubkey() {
                account_ids.push(1); // dummy account id
                continue;
            }
            let account_info = validity_prover_client.get_account_info(*pubkey).await?;
            if account_info.account_id.is_none() {
                return Err(BlockBuilderError::AccountNotFound(*pubkey));
            }
            account_ids.push(account_info.account_id.unwrap());
        }
        account_id_packed = Some(AccountIdPacked::pack(&account_ids));
    }
    let account_id_hash = account_id_packed.map_or(Bytes32::default(), |ids| ids.hash());
    let mut sender_with_signatures = block_post
        .pubkeys
        .iter()
        .map(|pubkey| SenderWithSignature {
            sender: *pubkey,
            signature: None,
        })
        .collect::<Vec<_>>();
    for signature in block_post.signatures.iter() {
        if eliminated_pubkeys.contains(&signature.pubkey) {
            // ignore eliminated pubkey
            continue;
        }
        let tx_index = block_post
            .pubkeys
            .iter()
            .position(|pubkey| pubkey == &signature.pubkey)
            .unwrap(); // safe
        sender_with_signatures[tx_index].signature = Some(signature.signature.clone());
    }
    let signature = construct_signature(
        block_post.tx_tree_root,
        block_post.expiry,
        block_post.pubkey_hash,
        account_id_hash,
        block_post.is_registration_block,
        &sender_with_signatures,
    );

    // call contract
    if block_post.is_registration_block {
        let trimmed_pubkeys = block_post
            .pubkeys
            .into_iter()
            .filter(|pubkey| !pubkey.is_dummy_pubkey())
            .collect::<Vec<_>>();
        rollup_contract
            .post_registration_block(
                block_builder_private_key,
                eth_allowance_for_block,
                block_post.tx_tree_root,
                block_post.expiry,
                signature.sender_flag,
                signature.agg_pubkey,
                signature.agg_signature,
                signature.message_point,
                trimmed_pubkeys,
            )
            .await?;
    } else {
        rollup_contract
            .post_non_registration_block(
                block_builder_private_key,
                eth_allowance_for_block,
                block_post.tx_tree_root,
                block_post.expiry,
                signature.sender_flag,
                signature.agg_pubkey,
                signature.agg_signature,
                signature.message_point,
                block_post.pubkey_hash,
                account_id_packed.unwrap().to_trimmed_bytes(),
            )
            .await?;
    };

    Ok(())
}
