use intmax2_interfaces::api::block_builder::interface::FeeProof;
use intmax2_zkp::{
    common::{
        block_builder::BlockProposal,
        signature::{block_sign_payload::BlockSignPayload, utils::get_pubkey_hash},
        trees::tx_tree::TxTree,
        tx::Tx,
    },
    constants::{NUM_SENDERS_IN_BLOCK, TX_TREE_HEIGHT},
    ethereum_types::{
        account_id::{AccountId, AccountIdPacked},
        address::Address,
        bytes32::Bytes32,
        u256::U256,
    },
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxRequest {
    pub request_id: String,
    pub pubkey: U256,
    pub account_id: Option<AccountId>,
    pub tx: Tx,
    pub fee_proof: Option<FeeProof>,
}

impl Default for TxRequest {
    fn default() -> Self {
        Self {
            request_id: Uuid::default().to_string(),
            pubkey: U256::dummy_pubkey(),
            account_id: Some(AccountId::dummy()),
            tx: Tx::default(),
            fee_proof: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposalMemo {
    pub created_at: u64,
    pub block_id: String,
    pub block_sign_payload: BlockSignPayload,
    pub pubkeys: Vec<U256>,            // sorted & padded pubkeys
    pub pubkey_hash: Bytes32,          // hash of the sorted & padded pubkeys
    pub tx_requests: Vec<TxRequest>,   // not sorted tx requests
    pub proposals: Vec<BlockProposal>, // proposals in the order of the tx requests
}

impl ProposalMemo {
    pub fn from_tx_requests(
        is_registration_block: bool,
        block_builder_address: Address,
        block_builder_nonce: u32,
        tx_requests: &[TxRequest],
        tx_timeout: u64,
    ) -> Self {
        let expiry = tx_timeout + chrono::Utc::now().timestamp() as u64;
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

        let block_sign_payload = BlockSignPayload {
            is_registration_block,
            tx_tree_root,
            expiry: expiry.into(),
            block_builder_address,
            block_builder_nonce,
        };
        let mut proposals = Vec::new();
        for r in tx_requests {
            let pubkey = r.pubkey;
            let tx_index = sorted_and_padded_txs
                .iter()
                .position(|r| r.pubkey == pubkey)
                .unwrap() as u32;
            let tx_merkle_proof = tx_tree.prove(tx_index as u64);
            proposals.push(BlockProposal {
                block_sign_payload: block_sign_payload.clone(),
                tx_index,
                tx_merkle_proof,
                pubkeys: pubkeys.clone(),
                pubkeys_hash: pubkey_hash,
            });
        }
        ProposalMemo {
            block_sign_payload,
            pubkeys,
            pubkey_hash,
            tx_requests: tx_requests.to_vec(),
            proposals,
            created_at: chrono::Utc::now().timestamp() as u64,
            block_id: Uuid::new_v4().to_string(),
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
    fn get_account_id(&self, pubkey: U256) -> Option<AccountId> {
        if pubkey == U256::dummy_pubkey() {
            return Some(AccountId::dummy());
        }
        self.tx_requests
            .iter()
            .find(|r| r.pubkey == pubkey)
            .and_then(|r| r.account_id)
    }

    // get the account ids for the tx requests in the memo
    pub fn get_account_ids(&self) -> Option<AccountIdPacked> {
        if self.block_sign_payload.is_registration_block {
            None
        } else {
            let account_ids: Vec<AccountId> = self
                .pubkeys
                .iter()
                .map(|pubkey| self.get_account_id(*pubkey).unwrap())
                .collect();
            Some(AccountIdPacked::pack(&account_ids))
        }
    }
}
