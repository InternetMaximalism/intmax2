use ethers::types::H256;
use intmax2_client_sdk::external_api::{
    contract::rollup_contract::RollupContract, validity_prover::ValidityProverClient,
};
use intmax2_interfaces::api::validity_prover::interface::ValidityProverClientInterface;
use intmax2_zkp::{
    common::{
        block_builder::{construct_signature, SenderWithSignature, UserSignature},
        signature_content::block_sign_payload::BlockSignPayload,
    },
    constants::NUM_SENDERS_IN_BLOCK,
    ethereum_types::{
        account_id::{AccountId, AccountIdPacked},
        bytes32::Bytes32,
        u256::U256,
    },
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{error::BlockBuilderError, types::ProposalMemo};

const PENALTY_FEE_POLLING_INTERVAL: u64 = 2;
const VALIDITY_PROVER_SYNC_POLLING_INTERVAL: u64 = 5;
const VALIDITY_SYNC_MAX_RETRY: u64 = 10;
const EXPIRY_BUFFER: u64 = 5;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockPostTask {
    pub force_post: bool,
    pub block_sign_payload: BlockSignPayload,
    pub pubkeys: Vec<U256>, // sorted & padded pubkeys
    pub account_ids: Option<AccountIdPacked>,
    pub pubkey_hash: Bytes32,
    pub signatures: Vec<UserSignature>,
    pub block_id: String,
}

impl Default for BlockPostTask {
    // empty block task
    fn default() -> Self {
        Self {
            force_post: true,
            block_sign_payload: BlockSignPayload::default(),
            pubkeys: vec![U256::dummy_pubkey(); NUM_SENDERS_IN_BLOCK],
            account_ids: Some(AccountIdPacked::pack(
                &[AccountId::dummy(); NUM_SENDERS_IN_BLOCK],
            )),
            pubkey_hash: Bytes32::default(),
            signatures: Vec::new(),
            block_id: Uuid::new_v4().to_string(),
        }
    }
}

impl BlockPostTask {
    pub fn from_memo(memo: &ProposalMemo, signatures: &[UserSignature]) -> Self {
        Self {
            force_post: false,
            block_sign_payload: memo.block_sign_payload.clone(),
            pubkeys: memo.pubkeys.clone(),
            account_ids: memo.get_account_ids(),
            pubkey_hash: memo.pubkey_hash,
            signatures: signatures.to_vec(),
            block_id: memo.block_id.clone(),
        }
    }
}

pub(crate) async fn post_block(
    block_builder_private_key: H256,
    gas_limit: Option<u64>,
    eth_allowance_for_block: U256,
    rollup_contract: &RollupContract,
    validity_prover_client: &ValidityProverClient,
    block_post: BlockPostTask,
) -> Result<(), BlockBuilderError> {
    log::info!(
        "Posting block: is_registration_block={}, tx_tree_root={}, expiry={}, num_signatures={}, force_post={}",
        block_post.block_sign_payload.is_registration_block,
        block_post.block_sign_payload.tx_tree_root,
        block_post.block_sign_payload.expiry,
        block_post.signatures.len(),
        block_post.force_post
    );

    if block_post.signatures.is_empty() && !block_post.force_post {
        log::warn!("No signatures in the block. Skipping post.");
        return Ok(());
    }

    // wait until validity prover syncs
    let mut retry = 0;
    loop {
        let onchain_latest_block_number = rollup_contract.get_latest_block_number().await?;
        let validity_prover_latest_block_number = validity_prover_client.get_block_number().await?;
        // break if synced
        if onchain_latest_block_number == validity_prover_latest_block_number {
            break;
        }
        if retry >= VALIDITY_SYNC_MAX_RETRY {
            log::error!("Validity prover is not synced after {} retries", retry);
            return Err(BlockBuilderError::ValidityProverIsNotSynced(
                onchain_latest_block_number,
                validity_prover_latest_block_number,
            ));
        }
        retry += 1;
        log::warn!(
            "Validity prover is not synced: onchain={}, validity_prover={}",
            onchain_latest_block_number,
            validity_prover_latest_block_number
        );
        tokio::time::sleep(tokio::time::Duration::from_secs(
            VALIDITY_PROVER_SYNC_POLLING_INTERVAL,
        ))
        .await;
    }

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

    // expiry check
    let current_time = chrono::Utc::now().timestamp() as u64;
    let expiry: u64 = block_post.block_sign_payload.expiry.into();
    if expiry != 0 && expiry < current_time + EXPIRY_BUFFER {
        log::error!(
            "Block already expired: expiry={}, current_time={}, buffer={}",
            expiry,
            current_time,
            EXPIRY_BUFFER
        );
        return Err(BlockBuilderError::AlreadyExpired);
    }

    // construct signature
    let mut account_id_packed = None;
    let mut eliminated_pubkeys = Vec::new();
    if block_post.block_sign_payload.is_registration_block {
        // eliminate pubkeys that already have account_id, which means the user sent another registration tx before this block
        // filter out dummy pubkeys for efficiency
        let pubkeys_without_dummy = block_post
            .pubkeys
            .iter()
            .filter(|pubkey| !pubkey.is_dummy_pubkey())
            .cloned()
            .collect::<Vec<_>>();
        let account_ids = validity_prover_client
            .get_account_info_batch(&pubkeys_without_dummy)
            .await?;
        for (pubkey, account_info) in pubkeys_without_dummy.iter().zip(account_ids.iter()) {
            if account_info.account_id.is_some() {
                eliminated_pubkeys.push(*pubkey);
            }
        }
    } else {
        let account_ids = block_post.account_ids.expect("account_ids is not set");
        account_id_packed = Some(account_ids);
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
        &block_post.block_sign_payload,
        block_post.pubkey_hash,
        account_id_hash,
        &sender_with_signatures,
    );

    // call contract
    if block_post.block_sign_payload.is_registration_block {
        let trimmed_pubkeys = block_post
            .pubkeys
            .into_iter()
            .filter(|pubkey| !pubkey.is_dummy_pubkey())
            .collect::<Vec<_>>();
        rollup_contract
            .post_registration_block(
                block_builder_private_key,
                gas_limit,
                eth_allowance_for_block,
                block_post.block_sign_payload.tx_tree_root,
                block_post.block_sign_payload.expiry.into(),
                block_post.block_sign_payload.block_builder_nonce,
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
                gas_limit,
                eth_allowance_for_block,
                block_post.block_sign_payload.tx_tree_root,
                block_post.block_sign_payload.expiry.into(),
                block_post.block_sign_payload.block_builder_nonce,
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
