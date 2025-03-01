use intmax2_zkp::{
    common::block_builder::UserSignature,
    ethereum_types::{account_id_packed::AccountIdPacked, bytes32::Bytes32, u256::U256},
};
use serde::{Deserialize, Serialize};

use crate::app::types::ProposalMemo;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposingBlockState {
    pub memo: ProposalMemo,
    pub signatures: Vec<UserSignature>,
}

impl ProposingBlockState {
    pub fn to_block_post_task(&self, force_post: bool) -> BlockPostTask {
        BlockPostTask {
            force_post,
            is_registration_block: self.memo.is_registration_block,
            tx_tree_root: self.memo.tx_tree_root,
            expiry: self.memo.expiry,
            pubkeys: self.memo.pubkeys.clone(),
            account_ids: self.memo.get_account_ids(),
            pubkey_hash: self.memo.pubkey_hash,
            signatures: self.signatures.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockPostTask {
    pub force_post: bool,
    pub is_registration_block: bool,
    pub tx_tree_root: Bytes32,
    pub expiry: u64,
    pub pubkeys: Vec<U256>, // sorted & padded pubkeys
    pub account_ids: Option<AccountIdPacked>,
    pub pubkey_hash: Bytes32,
    pub signatures: Vec<UserSignature>,
}
