use intmax2_interfaces::api::block_builder::interface::BlockBuilderStatus;
use intmax2_zkp::{
    common::{
        block_builder::{BlockProposal, UserSignature},
        signature::utils::get_pubkey_hash,
        trees::tx_tree::TxTree,
        tx::Tx,
    },
    constants::{NUM_SENDERS_IN_BLOCK, TX_TREE_HEIGHT},
    ethereum_types::{bytes32::Bytes32, u256::U256},
};

#[derive(Default, Debug, Clone)]
pub enum BuilderState {
    #[default]
    Pausing, // not accepting tx requests
    AcceptingTxs(AcceptingTxState),      // accepting  tx request
    ProposingBlock(ProposingBlockState), // after constructed the block, accepting signatures for the block
}

#[derive(Debug, Clone)]
pub struct AcceptingTxState {
    tx_requests: Vec<(U256, Tx)>, // hold in the order the request came
}

#[derive(Debug, Clone)]
pub struct ProposingBlockState {
    memo: ProposalMemo,
    signatures: Vec<UserSignature>,
}

#[derive(Debug, Clone)]
pub struct ProposalMemo {
    pub tx_tree_root: Bytes32,
    pub expiry: u64,
    pub pubkeys: Vec<U256>,            // sorted & padded pubkeys
    pub pubkey_hash: Bytes32,          // hash of the sorted & padded pubkeys
    pub tx_requests: Vec<(U256, Tx)>,  // not sorted tx requests
    pub proposals: Vec<BlockProposal>, // proposals in the order of the tx requests
}

impl ProposalMemo {
    pub fn get_proposal(&self, pubkey: U256, tx: Tx) -> Option<BlockProposal> {
        let position = self
            .tx_requests
            .iter()
            .position(|(p, t)| *p == pubkey && *t == tx);
        position.map(|pos| self.proposals[pos].clone())
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
                .any(|(p, t)| p == &pubkey && t == &tx),
            _ => false,
        }
    }

    pub fn is_pubkey_contained(&self, pubkey: U256) -> bool {
        match self {
            BuilderState::AcceptingTxs(state) => {
                state.tx_requests.iter().any(|(p, _)| p == &pubkey)
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
    pub fn append_tx_request(&mut self, pubkey: U256, tx: Tx) {
        match self {
            BuilderState::AcceptingTxs(state) => {
                state.tx_requests.push((pubkey, tx));
            }
            _ => panic!("Invalid state transition"),
        }
    }

    /// Propose a block with the tx requests
    pub fn propose_block(&mut self) {
        // todo: set
        let expiry = 0;

        let tx_requests = match self {
            BuilderState::AcceptingTxs(state) => state.tx_requests.clone(),
            _ => panic!("Invalid state transition"),
        };

        let mut sorted_and_padded_txs = tx_requests.clone();
        sorted_and_padded_txs.sort_by(|a, b| b.0.cmp(&a.0));
        sorted_and_padded_txs.resize(NUM_SENDERS_IN_BLOCK, (U256::dummy_pubkey(), Tx::default()));

        let pubkeys = sorted_and_padded_txs
            .iter()
            .map(|tx| tx.0)
            .collect::<Vec<_>>();
        let pubkey_hash = get_pubkey_hash(&pubkeys);

        let mut tx_tree = TxTree::new(TX_TREE_HEIGHT);
        for (_, tx) in sorted_and_padded_txs.iter() {
            tx_tree.push(*tx);
        }
        let tx_tree_root: Bytes32 = tx_tree.get_root().into();

        let mut proposals = Vec::new();
        for (pubkey, _tx) in tx_requests.iter() {
            let tx_index = sorted_and_padded_txs
                .iter()
                .position(|(p, _)| p == pubkey)
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
