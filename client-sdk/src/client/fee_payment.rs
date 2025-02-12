use intmax2_interfaces::{
    api::{
        block_builder::interface::Fee, store_vault_server::interface::StoreVaultClientInterface,
        validity_prover::interface::ValidityProverClientInterface,
    },
    data::encryption::Encryption,
};
use intmax2_zkp::{
    common::{generic_address::GenericAddress, signature::key_set::KeySet, transfer::Transfer},
    ethereum_types::{address::Address, u256::U256},
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::client::{
    misc::{get_topic, payment_memo::PaymentMemo},
    sync::utils::generate_salt,
};

use super::{client::PaymentMemoEntry, error::ClientError};

pub const WITHDRAWAL_FEE_MEMO: &str = "withdrawal_fee_memo";
pub const CLAIM_FEE_MEMO: &str = "claim_fee_memo";
pub const USED_OR_INVALID_MEMO: &str = "used_or_invalid_memo";

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WithdrawalFeeMemo {
    pub withdrawal_transfer: Transfer,
    pub fee: Fee,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimFeeMemo {
    pub fee: Fee,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsedOrInvalidMemo {
    pub reason: String,
}

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

pub enum FeeType {
    Withdrawal,
    Claim,
}

pub async fn get_unused_payments<
    S: StoreVaultClientInterface,
    V: ValidityProverClientInterface,
    M: DeserializeOwned,
>(
    store_vault_server: &S,
    key: KeySet,
    fee_type: FeeType,
) -> Result<Vec<PaymentMemo>, ClientError> {
    let topic = match fee_type {
        FeeType::Withdrawal => get_topic(WITHDRAWAL_FEE_MEMO),
        FeeType::Claim => get_topic(CLAIM_FEE_MEMO),
    };
    let encrypted_memos = store_vault_server
        .get_misc_sequence(key, topic, &None)
        .await?;
    if encrypted_memos.is_empty() {
        // early return if no memos
        return Ok(vec![]);
    }

    let memos = encrypted_memos
        .iter()
        .map(|data| PaymentMemo::decrypt(&data.data, key))
        .collect::<Result<Vec<PaymentMemo>, _>>()?;
    let used_topic = get_topic(USED_OR_INVALID_MEMO);
    let encrypted_used_memos = store_vault_server
        .get_misc_sequence(key, used_topic, &None)
        .await?;
    let used_memos = encrypted_used_memos
        .iter()
        .map(|data| PaymentMemo::decrypt(&data.data, key))
        .collect::<Result<Vec<PaymentMemo>, _>>()?;
    let unused_memos = memos
        .into_iter()
        .filter(|memo| {
            !used_memos
                .iter()
                .any(|used_memo| used_memo.transfer_uuid == memo.transfer_uuid)
        })
        .collect::<Vec<PaymentMemo>>();
    Ok(unused_memos)
}

pub async fn consume_payment<
    S: StoreVaultClientInterface,
    V: ValidityProverClientInterface,
    M: DeserializeOwned,
>(
    store_vault_server: &S,
    key: KeySet,
    payment_memo: &PaymentMemo,
    reason: &str,
) -> Result<(), ClientError> {
    let topic = get_topic(USED_OR_INVALID_MEMO);
    let memo = UsedOrInvalidMemo {
        reason: reason.to_string(),
    };
    let payment_memo = PaymentMemo {
        transfer_uuid: payment_memo.transfer_uuid.clone(),
        transfer: payment_memo.transfer,
        memo: serde_json::to_string(&memo).unwrap(),
    };
    store_vault_server
        .save_misc(key, topic, &payment_memo.encrypt(key.pubkey))
        .await?;
    Ok(())
}
