use intmax2_zkp::{
    common::{
        block_builder::{BlockProposal, UserSignature},
        signature::utils::get_pubkey_hash,
        trees::tx_tree::TxTree,
    },
    constants::{NUM_SENDERS_IN_BLOCK, TX_TREE_HEIGHT},
    ethereum_types::{account_id_packed::AccountIdPacked, bytes32::Bytes32, u256::U256},
};
use serde::{Deserialize, Serialize};

use crate::app::types::{ProposalMemo, TxRequest};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcceptingTxState {
    pub is_registration_block: bool,
    pub block_id: String,
    pub tx_requests: Vec<TxRequest>, // hold in the order the request came
}

impl AcceptingTxState {
    pub fn to_proposal_memo(&self, expiry: u64) -> ProposalMemo {
        let mut sorted_and_padded_txs = self.tx_requests.clone();
        sorted_and_padded_txs.sort_by(|a, b| b.pubkey.cmp(&a.pubkey));
        sorted_and_padded_txs.resize(NUM_SENDERS_IN_BLOCK, TxRequest::default());

        let pubkeys = sorted_and_padded_txs
            .iter()
            .map(|tx| tx.pubkey)
            .collect::<Vec<_>>();
        let pubkey_hash = get_pubkey_hash(&pubkeys);

        let mut tx_tree = TxTree::new(TX_TREE_HEIGHT);
        for r in sorted_and_padded_txs.iter() {
            tx_tree.push(r.tx);
        }
        let tx_tree_root: Bytes32 = tx_tree.get_root().into();

        let mut proposals = Vec::new();
        for r in self.tx_requests.iter() {
            let pubkey = r.pubkey;
            let tx_index = sorted_and_padded_txs
                .iter()
                .position(|r| r.pubkey == pubkey)
                .unwrap() as u32;
            let tx_merkle_proof = tx_tree.prove(tx_index as u64);
            proposals.push(BlockProposal {
                tx_tree_root,
                expiry,
                tx_index,
                tx_merkle_proof,
                pubkeys: pubkeys.clone(),
                pubkeys_hash: pubkey_hash,
            });
        }
        ProposalMemo {
            is_registration_block: self.is_registration_block,
            tx_tree_root,
            expiry,
            pubkeys,
            pubkey_hash,
            tx_requests: self.tx_requests.clone(),
            proposals,
        }
    }
}

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
