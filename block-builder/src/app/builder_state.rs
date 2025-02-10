use intmax2_interfaces::api::block_builder::interface::{BlockBuilderStatus, FeeProof};
use intmax2_zkp::{
    common::{
        block_builder::{BlockProposal, UserSignature},
        signature::utils::get_pubkey_hash,
        trees::tx_tree::TxTree,
        tx::Tx,
    },
    constants::{NUM_SENDERS_IN_BLOCK, TX_TREE_HEIGHT},
    ethereum_types::{account_id_packed::AccountIdPacked, bytes32::Bytes32, u256::U256},
};
use serde::{Deserialize, Serialize};

#[derive(Default, Debug, Clone)]
pub enum BuilderState {
    #[default]
    Pausing, // not accepting tx requests
    AcceptingTxs(AcceptingTxState),      // accepting  tx request
    ProposingBlock(ProposingBlockState), // after constructed the block, accepting signatures for the block
}

#[derive(Debug, Clone)]
pub struct AcceptingTxState {
    tx_requests: Vec<TxRequest>, // hold in the order the request came
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxRequest {
    pub pubkey: U256,
    pub account_id: Option<u64>,
    pub tx: Tx,
    pub fee_proof: Option<FeeProof>,
}

impl Default for TxRequest {
    fn default() -> Self {
        Self {
            pubkey: U256::dummy_pubkey(),
            account_id: Some(1), // account id of dummy pubkey is 1
            tx: Tx::default(),
            fee_proof: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProposingBlockState {
    memo: ProposalMemo,
    signatures: Vec<UserSignature>,
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
    pub fn get_proposal(&self, pubkey: U256, tx: Tx) -> Option<BlockProposal> {
        let position = self
            .tx_requests
            .iter()
            .position(|r| r.pubkey == pubkey && r.tx == tx);
        position.map(|pos| self.proposals[pos].clone())
    }

    fn get_account_id(&self, pubkey: U256) -> Option<u64> {
        if pubkey == U256::dummy_pubkey() {
            return Some(1);
        }
        self.tx_requests
            .iter()
            .find(|r| r.pubkey == pubkey)
            .and_then(|r| r.account_id)
    }

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

impl BuilderState {
    pub fn get_status(&self) -> BlockBuilderStatus {
        match self {
            BuilderState::Pausing => BlockBuilderStatus::Pausing,
            BuilderState::AcceptingTxs(_) => BlockBuilderStatus::AcceptingTxs,
            BuilderState::ProposingBlock(_) => BlockBuilderStatus::ProposingBlock,
        }
    }

    pub fn is_pausing(&self) -> bool {
        matches!(self, BuilderState::Pausing)
    }

    pub fn is_accepting_txs(&self) -> bool {
        matches!(self, BuilderState::AcceptingTxs(_))
    }

    pub fn is_proposing_block(&self) -> bool {
        matches!(self, BuilderState::ProposingBlock(_))
    }

    pub fn is_request_contained(&self, pubkey: U256, tx: Tx) -> bool {
        match self {
            BuilderState::AcceptingTxs(state) => state
                .tx_requests
                .iter()
                .any(|r| r.pubkey == pubkey && r.tx == tx),
            _ => false,
        }
    }

    pub fn is_pubkey_contained(&self, pubkey: U256) -> bool {
        match self {
            BuilderState::AcceptingTxs(state) => {
                state.tx_requests.iter().any(|r| r.pubkey == pubkey)
            }
            _ => false,
        }
    }

    pub fn get_proposal_memo(&self) -> Option<ProposalMemo> {
        match self {
            BuilderState::ProposingBlock(state) => Some(state.memo.clone()),
            _ => None,
        }
    }

    pub fn get_signatures(&self) -> Option<Vec<UserSignature>> {
        match self {
            BuilderState::ProposingBlock(state) => Some(state.signatures.clone()),
            _ => None,
        }
    }

    pub fn count_tx_requests(&self) -> usize {
        match self {
            BuilderState::AcceptingTxs(state) => state.tx_requests.len(),
            _ => 0,
        }
    }

    pub fn start_accepting_txs(&mut self) {
        match self {
            BuilderState::Pausing => {
                *self = BuilderState::AcceptingTxs(AcceptingTxState {
                    tx_requests: Vec::new(),
                });
            }
            _ => panic!("Invalid state transition"),
        }
    }

    /// Accept tx request
    pub fn append_tx_request(
        &mut self,
        pubkey: U256,
        account_id: Option<u64>,
        tx: Tx,
        fee_proof: Option<FeeProof>,
    ) {
        match self {
            BuilderState::AcceptingTxs(state) => {
                state.tx_requests.push(TxRequest {
                    pubkey,
                    account_id,
                    tx,
                    fee_proof,
                });
            }
            _ => panic!("Invalid state transition"),
        }
    }

    /// Propose a block with the tx requests
    pub fn propose_block(&mut self, is_registration_block: bool) {
        // todo: set
        let expiry = 0;

        let tx_requests = match self {
            BuilderState::AcceptingTxs(state) => state.tx_requests.clone(),
            _ => panic!("Invalid state transition"),
        };

        let mut sorted_and_padded_txs = tx_requests.clone();
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
        for r in tx_requests.iter() {
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

        let memo = ProposalMemo {
            is_registration_block,
            tx_tree_root,
            expiry,
            pubkeys,
            pubkey_hash,
            tx_requests,
            proposals,
        };

        *self = BuilderState::ProposingBlock(ProposingBlockState {
            memo,
            signatures: Vec::new(),
        });
    }

    pub fn append_signature(&mut self, signature: UserSignature) {
        match self {
            BuilderState::ProposingBlock(state) => {
                state.signatures.push(signature);
            }
            _ => panic!("Invalid state transition"),
        }
    }

    pub fn finalize_block(&mut self) {
        match self {
            BuilderState::ProposingBlock(_) => {
                *self = BuilderState::Pausing;
            }
            _ => panic!("Invalid state transition"),
        }
    }

    pub fn query_proposal(&self, pubkey: U256, tx: Tx) -> Option<BlockProposal> {
        match self {
            BuilderState::ProposingBlock(state) => state.memo.get_proposal(pubkey, tx),
            _ => None,
        }
    }
}
