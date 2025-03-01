use intmax2_interfaces::api::block_builder::interface::FeeProof;
use intmax2_zkp::{
    common::{block_builder::BlockProposal, signature::utils::get_pubkey_hash, trees::tx_tree::TxTree, tx::Tx},
    constants::{NUM_SENDERS_IN_BLOCK, TX_TREE_HEIGHT},
    ethereum_types::{account_id_packed::AccountIdPacked, bytes32::Bytes32, u256::U256},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxRequest {
    pub request_id: String,
    pub pubkey: U256,
    pub account_id: Option<u64>,
    pub tx: Tx,
    pub fee_proof: Option<FeeProof>,
}

impl Default for TxRequest {
    fn default() -> Self {
        Self {
            request_id: Uuid::default().to_string(),
            pubkey: U256::dummy_pubkey(),
            account_id: Some(1), // account id of dummy pubkey is 1
            tx: Tx::default(),
            fee_proof: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposalMemo {
    pub is_registration_block: bool,
    pub tx_tree_root: Bytes32,
    pub expiry: u64,
    pub pubkeys: Vec<U256>,            // sorted & padded pubkeys
    pub pubkey_hash: Bytes32,          // hash of the sorted & padded pubkeys
    pub tx_requests: Vec<TxRequest>,   // not sorted tx requests
    pub proposals: Vec<BlockProposal>, // proposals in the order of the tx requests
}

impl ProposalMemo {
    pub fn from_tx_requests(
        is_registration_block: bool,
        tx_requests: &[TxRequest],
        expiry: u64,
    ) -> Self {
        let mut sorted_and_padded_txs = tx_requests.to_vec();
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
        for r in tx_requests {
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
            is_registration_block,
            tx_tree_root,
            expiry,
            pubkeys,
            pubkey_hash,
            tx_requests: tx_requests.to_vec(),
            proposals,
        }
    }

    // get the proposal for a given pubkey and tx if it exists
    pub fn get_proposal(&self, pubkey: U256, tx: Tx) -> Option<BlockProposal> {
        let position = self
            .tx_requests
            .iter()
            .position(|r| r.pubkey == pubkey && r.tx == tx);
        position.map(|pos| self.proposals[pos].clone())
    }

    // get the account id for a given pubkey
    fn get_account_id(&self, pubkey: U256) -> Option<u64> {
        if pubkey == U256::dummy_pubkey() {
            return Some(1);
        }
        self.tx_requests
            .iter()
            .find(|r| r.pubkey == pubkey)
            .and_then(|r| r.account_id)
    }

    // get the account ids for the tx requests in the memo
    pub fn get_account_ids(&self) -> Option<AccountIdPacked> {
        if self.is_registration_block {
            None
        } else {
            let account_ids: Vec<u64> = self
                .pubkeys
                .iter()
                .map(|pubkey| self.get_account_id(*pubkey).unwrap())
                .collect();
            Some(AccountIdPacked::pack(&account_ids))
        }
    }
}
