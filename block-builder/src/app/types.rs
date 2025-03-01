use intmax2_interfaces::api::block_builder::interface::FeeProof;
use intmax2_zkp::{
    common::{block_builder::BlockProposal, tx::Tx},
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
