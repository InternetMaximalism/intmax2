use intmax2_interfaces::api::block_builder::interface::FeeProof;
use intmax2_zkp::{
    common::{
        block_builder::BlockProposal,
        signature_content::{block_sign_payload::BlockSignPayload, utils::get_pubkey_hash},
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
    /// Create proposal memo for the provided tx requests.
    ///
    /// # Arguments
    /// * `is_registration_block` - Bool value if the block should be the registration one
    /// * `block_builder_address` - Block builder address
    /// * `block_builder_nonce` - Block builder nonce
    /// * `tx_requests` - Transaction requests. Its length should be in [0; NUM_SENDERS_IN_BLOCK)
    /// * `tx_timeout` - Transaction timeout
    pub fn from_tx_requests(
        is_registration_block: bool,
        block_builder_address: Address,
        block_builder_nonce: u32,
        tx_requests: &[TxRequest],
        tx_timeout: u64,
    ) -> Self {
        assert!(
            tx_requests.len() <= NUM_SENDERS_IN_BLOCK,
            "tx_requests.len() = {}, which exceeds NUM_SENDERS_IN_BLOCK = {}",
            tx_requests.len(),
            NUM_SENDERS_IN_BLOCK
        );

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

    /// Get the proposal for a given pubkey and tx if it exists.
    ///
    /// # Arguments
    /// * `pubkey` - Public key to be compared
    /// * `tx` - Transaction to be compared
    pub fn get_proposal(&self, pubkey: U256, tx: Tx) -> Option<BlockProposal> {
        let position = self
            .tx_requests
            .iter()
            .position(|r| r.pubkey == pubkey && r.tx == tx);
        position.map(|pos| self.proposals[pos].clone())
    }

    /// Get the account id for a given pubkey.
    ///
    /// # Arguments
    /// * `pubkey` - Given public key
    fn get_account_id(&self, pubkey: U256) -> Option<AccountId> {
        if pubkey == U256::dummy_pubkey() {
            return Some(AccountId::dummy());
        }
        self.tx_requests
            .iter()
            .find(|r| r.pubkey == pubkey)
            .and_then(|r| r.account_id)
    }

    /// Get the account ids for the tx requests in the memo.
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

#[cfg(test)]
mod tests {
    use super::*;
    use intmax2_zkp::{
        common::tx::Tx, constants::NUM_SENDERS_IN_BLOCK, ethereum_types::u256::U256,
    };

    use num_bigint::BigUint;

    #[test]
    fn test_basic_from_tx_requests() {
        let pubkey = U256::from(1);
        let tx = Tx::default();
        let tx_requests = vec![TxRequest {
            pubkey,
            tx,
            ..Default::default()
        }];

        let memo = ProposalMemo::from_tx_requests(false, Address::default(), 0, &tx_requests, 1000);

        assert_eq!(memo.tx_requests.len(), 1);
        assert_eq!(memo.pubkeys.len(), NUM_SENDERS_IN_BLOCK);
        assert_eq!(memo.proposals.len(), 1);
        assert_eq!(memo.pubkeys[0], pubkey);
    }

    #[test]
    fn test_pubkey_sorting() {
        let tx_requests = vec![
            TxRequest {
                pubkey: U256::from(5),
                ..Default::default()
            },
            TxRequest {
                pubkey: U256::from(10),
                ..Default::default()
            },
            TxRequest {
                pubkey: U256::from(1),
                ..Default::default()
            },
        ];

        let memo = ProposalMemo::from_tx_requests(false, Address::default(), 0, &tx_requests, 1000);

        let expected = [U256::from(10), U256::from(5), U256::from(1)];
        for (i, pk) in expected.iter().enumerate() {
            assert_eq!(memo.pubkeys[i], *pk);
        }
    }

    #[test]
    fn test_padding_to_num_senders() {
        let tx_requests = vec![TxRequest {
            pubkey: U256::from(2),
            ..Default::default()
        }];

        let memo = ProposalMemo::from_tx_requests(false, Address::default(), 0, &tx_requests, 1000);

        assert_eq!(memo.pubkeys.len(), NUM_SENDERS_IN_BLOCK);
        assert_eq!(memo.pubkeys[0], U256::from(2));
        for pk in memo.pubkeys.iter().skip(1) {
            assert_eq!(*pk, U256::dummy_pubkey());
        }
    }

    #[test]
    fn test_tx_index_proposal() {
        let tx_requests = vec![
            TxRequest {
                pubkey: U256::from(3),
                ..Default::default()
            },
            TxRequest {
                pubkey: U256::from(5),
                ..Default::default()
            },
        ];

        let memo = ProposalMemo::from_tx_requests(false, Address::default(), 0, &tx_requests, 1000);

        let proposal = &memo.proposals[0];
        assert_eq!(proposal.tx_index, 1);
    }

    #[test]
    fn test_registration_block_skips_account_ids() {
        let tx_requests = vec![TxRequest::default()];
        let memo = ProposalMemo::from_tx_requests(true, Address::default(), 0, &tx_requests, 1000);

        assert_eq!(memo.get_account_ids(), None);
    }

    #[test]
    fn test_get_proposal() {
        let pubkey = U256::from(42);
        let tx = Tx::default();
        let tx_requests = vec![TxRequest {
            pubkey,
            tx,
            ..Default::default()
        }];

        let memo = ProposalMemo::from_tx_requests(false, Address::default(), 0, &tx_requests, 1000);

        let proposal = memo.get_proposal(pubkey, tx);
        assert!(proposal.is_some());
        assert_eq!(proposal.unwrap().pubkeys[0], pubkey);
    }

    #[test]
    fn test_empty_tx_requests() {
        let memo = ProposalMemo::from_tx_requests(false, Address::default(), 0, &[], 1000);

        assert_eq!(memo.tx_requests.len(), 0);
        assert_eq!(memo.proposals.len(), 0);
        assert_eq!(memo.pubkeys.len(), NUM_SENDERS_IN_BLOCK);
        assert!(memo.pubkeys.iter().all(|p| *p == U256::dummy_pubkey()));
    }

    #[test]
    fn test_duplicate_pubkeys() {
        let pubkey = U256::from(123);
        let tx1 = Tx::default();
        let tx2 = Tx::default();

        let tx_requests = vec![
            TxRequest {
                pubkey,
                tx: tx1,
                ..Default::default()
            },
            TxRequest {
                pubkey,
                tx: tx2,
                ..Default::default()
            },
        ];

        let memo = ProposalMemo::from_tx_requests(false, Address::default(), 0, &tx_requests, 1000);

        assert_eq!(memo.tx_requests.len(), 2);
        assert_eq!(memo.proposals.len(), 2);

        let idx_0 = memo.proposals[0].tx_index;
        let idx_1 = memo.proposals[1].tx_index;

        assert_eq!(idx_0, idx_1);
    }

    #[test]
    fn test_full_tx_requests_in_block() {
        let tx_requests: Vec<TxRequest> = (0..(NUM_SENDERS_IN_BLOCK))
            .map(|i| TxRequest {
                pubkey: U256::try_from(BigUint::from(i)).unwrap(),
                ..Default::default()
            })
            .collect();

        let memo = ProposalMemo::from_tx_requests(false, Address::default(), 0, &tx_requests, 1000);

        // Proposals should be only for the original tx_requests
        assert_eq!(memo.proposals.len(), NUM_SENDERS_IN_BLOCK);

        // Pubkeys should be limited by NUM_SENDERS_IN_BLOCK
        assert_eq!(memo.pubkeys.len(), NUM_SENDERS_IN_BLOCK);

        // Check that no one pubkey is above the allowed value
        for pk in memo.pubkeys.iter() {
            assert!(pk <= &U256::try_from(BigUint::from(NUM_SENDERS_IN_BLOCK)).unwrap());
        }
    }

    #[test]
    #[should_panic(expected = "tx_requests.len()")]
    fn test_exceeding_tx_requests_in_block() {
        let tx_requests: Vec<TxRequest> = (0..(NUM_SENDERS_IN_BLOCK + 1))
            .map(|i| TxRequest {
                pubkey: U256::try_from(BigUint::from(i)).unwrap(),
                ..Default::default()
            })
            .collect();

        let _ = ProposalMemo::from_tx_requests(false, Address::default(), 0, &tx_requests, 1000);
    }
}
