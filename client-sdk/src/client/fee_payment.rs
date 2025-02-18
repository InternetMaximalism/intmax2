use intmax2_interfaces::{
    api::{
        block_builder::interface::Fee, store_vault_server::interface::StoreVaultClientInterface,
        validity_prover::interface::ValidityProverClientInterface,
        withdrawal_server::interface::WithdrawalServerClientInterface,
    },
    data::encryption::Encryption,
};
use intmax2_zkp::{
    common::{generic_address::GenericAddress, signature::key_set::KeySet, transfer::Transfer},
    ethereum_types::{u256::U256, u32limb_trait::U32LimbTrait as _},
};
use serde::{Deserialize, Serialize};

use crate::{
    client::{
        misc::{get_topic, payment_memo::PaymentMemo},
        receive_validation::validate_receive,
        sync::utils::generate_salt,
    },
    external_api::contract::withdrawal_contract::WithdrawalContract,
};

use super::{
    client::PaymentMemoEntry,
    receive_validation::ReceiveValidationError,
    sync::{error::SyncError, utils::quote_withdrawal_claim_fee},
};

pub const WITHDRAWAL_FEE_MEMO: &str = "withdrawal_fee_memo";
pub const CLAIM_FEE_MEMO: &str = "claim_fee_memo";
pub const USED_OR_INVALID_MEMO: &str = "used_or_invalid_memo";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FeeType {
    Withdrawal,
    Claim,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WithdrawalFeeMemo {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WithdrawalTransfers {
    pub transfers: Vec<Transfer>,
    pub withdrawal_fee_transfer_index: Option<u32>,
    pub claim_fee_transfer_index: Option<u32>,
}

/// quote withdrawal fee
pub(crate) async fn quote_withdrawal_fee<W: WithdrawalServerClientInterface>(
    withdrawal_server: &W,
    withdrawal_contract: &WithdrawalContract,
    withdrawal_token_index: u32,
    fee_token_index: u32,
) -> Result<(Option<U256>, Option<Fee>), SyncError> {
    let fee_info = withdrawal_server.get_withdrawal_fee().await?;
    let direct_withdrawal_indices = withdrawal_contract
        .get_direct_withdrawal_token_indices()
        .await?;
    let fees = if direct_withdrawal_indices.contains(&withdrawal_token_index) {
        fee_info.direct_withdrawal_fee.clone()
    } else {
        fee_info.claimable_withdrawal_fee.clone()
    };
    let fee = quote_withdrawal_claim_fee(Some(fee_token_index), fees)?;
    Ok((fee_info.beneficiary, fee))
}

/// quote claim fee
pub(crate) async fn quote_claim_fee<W: WithdrawalServerClientInterface>(
    withdrawal_server: &W,
    fee_token_index: u32,
) -> Result<(Option<U256>, Option<Fee>), SyncError> {
    let fee_info = withdrawal_server.get_claim_fee().await?;
    let fee = quote_withdrawal_claim_fee(Some(fee_token_index), fee_info.fee)?;
    Ok((fee_info.beneficiary, fee))
}

/// generate fee payment memos for withdrawal and claim fee
pub fn generate_fee_payment_memo(
    transfers: &[Transfer],
    withdrawal_fee_transfer_index: Option<u32>,
    claim_fee_transfer_index: Option<u32>,
) -> Result<Vec<PaymentMemoEntry>, SyncError> {
    let mut payment_memos = vec![];

    if let Some(withdrawal_fee_transfer_index) = withdrawal_fee_transfer_index {
        if withdrawal_fee_transfer_index >= transfers.len() as u32 {
            return Err(SyncError::FeeError(
                "withdrawal_fee_transfer_index is out of range".to_string(),
            ));
        }
        let fee_transfer = &transfers[withdrawal_fee_transfer_index as usize];
        let fee = Fee {
            token_index: fee_transfer.token_index,
            amount: fee_transfer.amount,
        };
        let withdrawal_fee_memo = WithdrawalFeeMemo { fee };
        let payment_memo = PaymentMemoEntry {
            transfer_index: withdrawal_fee_transfer_index,
            topic: get_topic(WITHDRAWAL_FEE_MEMO),
            memo: serde_json::to_string(&withdrawal_fee_memo).unwrap(),
        };
        payment_memos.push(payment_memo);
    }

    if let Some(claim_fee_transfer_index) = claim_fee_transfer_index {
        if claim_fee_transfer_index >= transfers.len() as u32 {
            return Err(SyncError::FeeError(
                "claim_fee_transfer_index is out of range".to_string(),
            ));
        }
        let fee_transfer = &transfers[claim_fee_transfer_index as usize];
        let fee = Fee {
            token_index: fee_transfer.token_index,
            amount: fee_transfer.amount,
        };
        let claim_fee_memo = ClaimFeeMemo { fee };
        let payment_memo = PaymentMemoEntry {
            transfer_index: claim_fee_transfer_index,
            topic: get_topic(CLAIM_FEE_MEMO),
            memo: serde_json::to_string(&claim_fee_memo).unwrap(),
        };
        payment_memos.push(payment_memo);
    }

    Ok(payment_memos)
}

/// quote fee and generate transfers for withdrawal and claim
pub async fn generate_withdrawal_transfers<W: WithdrawalServerClientInterface>(
    withdrawal_server: &W,
    withdrawal_contract: &WithdrawalContract,
    withdrawal_transfer: &Transfer,
    fee_token_index: u32,
    with_claim_fee: bool,
) -> Result<WithdrawalTransfers, SyncError> {
    let mut transfers = if withdrawal_transfer.amount == U256::zero() {
        // if withdrawal_transfer.amount is zero, ignore withdrawal_transfer
        // and only generate fee transfers
        vec![]
    } else {
        vec![*withdrawal_transfer]
    };

    let mut withdrawal_fee_transfer_index = None;
    let mut claim_fee_transfer_index = None;

    let (withdrawal_beneficiary, withdrawal_fee) = quote_withdrawal_fee(
        withdrawal_server,
        withdrawal_contract,
        withdrawal_transfer.token_index,
        fee_token_index,
    )
    .await?;
    if let Some(withdrawal_fee) = &withdrawal_fee {
        let withdrawal_beneficiary = withdrawal_beneficiary.ok_or(SyncError::FeeError(
            "withdrawal_beneficiary is not set".to_string(),
        ))?;
        let withdrawal_fee_transfer = Transfer {
            token_index: withdrawal_fee.token_index,
            recipient: GenericAddress::from_pubkey(withdrawal_beneficiary),
            amount: withdrawal_fee.amount,
            salt: generate_salt(),
        };
        withdrawal_fee_transfer_index = Some(transfers.len() as u32);
        transfers.push(withdrawal_fee_transfer);
    }
    if with_claim_fee {
        let (claim_beneficiary, claim_fee) =
            quote_claim_fee(withdrawal_server, fee_token_index).await?;
        if let Some(claim_fee) = claim_fee {
            let claim_beneficiary = claim_beneficiary.ok_or(SyncError::FeeError(
                "claim_beneficiary is not set".to_string(),
            ))?;
            let claim_fee_transfer = Transfer {
                token_index: claim_fee.token_index,
                recipient: GenericAddress::from_pubkey(claim_beneficiary),
                amount: claim_fee.amount,
                salt: generate_salt(),
            };
            claim_fee_transfer_index = Some(transfers.len() as u32);
            transfers.push(claim_fee_transfer);
        }
    }
    Ok(WithdrawalTransfers {
        transfers,
        withdrawal_fee_transfer_index,
        claim_fee_transfer_index,
    })
}

/// get unused payment memos
pub async fn get_unused_payments<S: StoreVaultClientInterface>(
    store_vault_server: &S,
    key: KeySet,
    fee_type: FeeType,
) -> Result<Vec<PaymentMemo>, SyncError> {
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
                .any(|used_memo| used_memo.meta.uuid == memo.meta.uuid)
        })
        .collect::<Vec<PaymentMemo>>();
    Ok(unused_memos)
}

/// consume payment memo
pub async fn consume_payment<S: StoreVaultClientInterface>(
    store_vault_server: &S,
    key: KeySet,
    payment_memo: &PaymentMemo,
    reason: &str,
) -> Result<(), SyncError> {
    let topic = get_topic(USED_OR_INVALID_MEMO);
    let memo = UsedOrInvalidMemo {
        reason: reason.to_string(),
    };
    let payment_memo = PaymentMemo {
        meta: payment_memo.meta.clone(),
        transfer_data: payment_memo.transfer_data.clone(),
        memo: serde_json::to_string(&memo).unwrap(),
    };
    store_vault_server
        .save_misc(key, topic, &payment_memo.encrypt(key.pubkey))
        .await?;
    Ok(())
}

/// select unused fees and validate them
pub async fn select_unused_fees<S: StoreVaultClientInterface, V: ValidityProverClientInterface>(
    store_vault_server: &S,
    validity_prover: &V,
    key: KeySet,
    fee_beneficiary: U256,
    fee: Fee,
    fee_type: FeeType,
    tx_timeout: u64,
) -> Result<Vec<PaymentMemo>, SyncError> {
    let unused_fees = get_unused_payments(store_vault_server, key, fee_type).await?;
    // Extract only those whose fee.token_index and recipient matches and sort by fee.amount
    let mut sorted_fee_memo = unused_fees
        .into_iter()
        .filter(|memo| {
            memo.transfer_data.transfer.token_index == fee.token_index
                && memo.transfer_data.transfer.recipient
                    == GenericAddress::from_pubkey(fee_beneficiary)
        })
        .collect::<Vec<_>>();
    sorted_fee_memo.sort_by_key(|memo| memo.transfer_data.transfer.amount);

    // Collect from the smallest to make the fee enough. If there is an invalid fee, mark it as consumed.
    let mut fee_transfers = vec![];
    let mut collected_total_fee = U256::zero();
    for memo in sorted_fee_memo {
        match validate_receive(
            store_vault_server,
            validity_prover,
            memo.transfer_data.transfer.recipient.to_pubkey().unwrap(),
            &memo.meta,
            &memo.transfer_data,
        )
        .await
        {
            Ok(transfer) => {
                fee_transfers.push(memo);
                collected_total_fee += transfer.amount;
            }
            Err(ReceiveValidationError::TxIsNotSettled(timestamp)) => {
                if timestamp + tx_timeout < chrono::Utc::now().timestamp() as u64 {
                    consume_payment(store_vault_server, key, &memo, "tx is timeout").await?;
                }
                return Err(SyncError::FeeError(format!(
                    "there is pending fee: timestamp: {}",
                    timestamp
                )));
            }
            Err(e) => {
                log::warn!("invalid fee: {}", e);
                consume_payment(store_vault_server, key, &memo, &e.to_string()).await?;
            }
        }
        if collected_total_fee >= fee.amount {
            break;
        }
    }
    if collected_total_fee < fee.amount {
        return Err(SyncError::FeeError(format!(
            "fee is not enough: collected_total_fee: {}, fee.amount: {}",
            collected_total_fee, fee.amount
        )));
    }
    Ok(fee_transfers)
}
