use intmax2_interfaces::api::block_builder::interface::Fee;
use intmax2_zkp::{
    common::{generic_address::GenericAddress, transfer::Transfer},
    ethereum_types::{address::Address, u256::U256},
};

use crate::client::{
    misc::{
        get_topic,
        payment_memo::{WithdrawalFeeMemo, WITHDRAWAL_FEE_MEMO},
    },
    sync::utils::generate_salt,
};

use super::{
    client::PaymentMemoEntry,
    misc::payment_memo::{ClaimFeeMemo, CLAIM_FEE_MEMO},
};

// Structure for transfer and payment memos used as input for send_tx_request
#[derive(Debug, Clone)]
pub struct TransfersWithMemo {
    pub transfers: Vec<Transfer>,
    pub payment_memos: Vec<PaymentMemoEntry>,
}

impl TransfersWithMemo {
    pub fn single_withdrawal_with_fee(
        recipient: Address,
        token_index: u32,
        amount: U256,
        fee_beneficiary: U256,
        fee: Fee,
    ) -> Self {
        let withdrawal_transfer = Transfer {
            recipient: GenericAddress::from_address(recipient),
            token_index,
            amount,
            salt: generate_salt(),
        };
        let fee_transfer = Transfer {
            recipient: GenericAddress::from_pubkey(fee_beneficiary),
            token_index: fee.token_index,
            amount: fee.amount,
            salt: generate_salt(),
        };
        let transfers = vec![withdrawal_transfer, fee_transfer];
        let withdrawal_fee_memo = WithdrawalFeeMemo {
            withdrawal_transfer,
            fee,
        };
        let payment_memo = PaymentMemoEntry {
            transfer_index: 1, // fee transfer index
            topic: get_topic(WITHDRAWAL_FEE_MEMO),
            memo: serde_json::to_string(&withdrawal_fee_memo).unwrap(),
        };
        let payment_memos = vec![payment_memo];

        TransfersWithMemo {
            transfers,
            payment_memos,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn single_withdrawal_with_claim_fee(
        recipient: Address,
        token_index: u32,
        amount: U256,
        withdrawal_fee_beneficiary: U256,
        withdrawal_fee: Fee,
        claim_fee_beneficiary: U256,
        claim_fee: Fee,
    ) -> Self {
        let withdrawal_transfer = Transfer {
            recipient: GenericAddress::from_address(recipient),
            token_index,
            amount,
            salt: generate_salt(),
        };
        let withdrawal_fee_transfer = Transfer {
            recipient: GenericAddress::from_pubkey(withdrawal_fee_beneficiary),
            token_index: withdrawal_fee.token_index,
            amount: withdrawal_fee.amount,
            salt: generate_salt(),
        };
        let claim_fee_transfer = Transfer {
            recipient: GenericAddress::from_pubkey(claim_fee_beneficiary),
            token_index: claim_fee.token_index,
            amount: claim_fee.amount,
            salt: generate_salt(),
        };
        let transfers = vec![
            withdrawal_transfer,
            withdrawal_fee_transfer,
            claim_fee_transfer,
        ];
        let withdrawal_fee_memo = WithdrawalFeeMemo {
            withdrawal_transfer,
            fee: withdrawal_fee,
        };
        let claim_fee_memo = ClaimFeeMemo { fee: claim_fee };
        let payment_memos = vec![
            PaymentMemoEntry {
                transfer_index: 1, // fee transfer index
                topic: get_topic(WITHDRAWAL_FEE_MEMO),
                memo: serde_json::to_string(&withdrawal_fee_memo).unwrap(),
            },
            PaymentMemoEntry {
                transfer_index: 2, // claim fee transfer index
                topic: get_topic(CLAIM_FEE_MEMO),
                memo: serde_json::to_string(&claim_fee_memo).unwrap(),
            },
        ];
        TransfersWithMemo {
            transfers,
            payment_memos,
        }
    }
}
