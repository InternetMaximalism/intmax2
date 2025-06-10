use intmax2_interfaces::{
    api::{
        block_builder::interface::Fee,
        store_vault_server::interface::{SaveDataEntry, StoreVaultClientInterface},
        validity_prover::interface::ValidityProverClientInterface,
        withdrawal_server::interface::WithdrawalServerClientInterface,
    },
    data::encryption::BlsEncryption,
    utils::{
        address::IntmaxAddress,
        key::{PublicKey, ViewPair},
    },
};
use intmax2_zkp::ethereum_types::{u256::U256, u32limb_trait::U32LimbTrait as _};
use serde::{Deserialize, Serialize};

use crate::{
    client::{misc::payment_memo::PaymentMemo, receive_validation::validate_receive, types::{GenericRecipient, PaymentMemoEntry, TransferRequest}},
    external_api::contract::withdrawal_contract::WithdrawalContract,
};

use super::{
    misc::payment_memo::{get_all_payment_memos, payment_memo_topic},
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
pub struct WithdrawalTransferRequests {
    pub transfer_requests: Vec<TransferRequest>,
    pub withdrawal_fee_transfer_index: Option<u32>,
    pub claim_fee_transfer_index: Option<u32>,
}

/// quote withdrawal fee
pub(crate) async fn quote_withdrawal_fee(
    withdrawal_server: &dyn WithdrawalServerClientInterface,
    withdrawal_contract: &WithdrawalContract,
    withdrawal_token_index: u32,
    fee_token_index: u32,
) -> Result<(IntmaxAddress, Option<Fee>), SyncError> {
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
pub(crate) async fn quote_claim_fee(
    withdrawal_server: &dyn WithdrawalServerClientInterface,
    fee_token_index: u32,
) -> Result<(IntmaxAddress, Option<Fee>), SyncError> {
    let fee_info = withdrawal_server.get_claim_fee().await?;
    let fee = quote_withdrawal_claim_fee(Some(fee_token_index), fee_info.fee)?;
    Ok((fee_info.beneficiary, fee))
}

/// generate fee payment memos for withdrawal and claim fee
pub fn generate_fee_payment_memo(
    transfer_requests: &[TransferRequest],
    withdrawal_fee_transfer_index: Option<u32>,
    claim_fee_transfer_index: Option<u32>,
) -> Result<Vec<PaymentMemoEntry>, SyncError> {
    let mut payment_memos = vec![];

    if let Some(withdrawal_fee_transfer_index) = withdrawal_fee_transfer_index {
        if withdrawal_fee_transfer_index >= transfer_requests.len() as u32 {
            return Err(SyncError::FeeError(
                "withdrawal_fee_transfer_index is out of range".to_string(),
            ));
        }
        let fee_transfer_req = &transfer_requests[withdrawal_fee_transfer_index as usize];
        let fee = Fee {
            token_index: fee_transfer_req.token_index,
            amount: fee_transfer_req.amount,
        };
        let withdrawal_fee_memo = WithdrawalFeeMemo { fee };
        let payment_memo = PaymentMemoEntry {
            transfer_index: withdrawal_fee_transfer_index,
            topic: payment_memo_topic(WITHDRAWAL_FEE_MEMO),
            memo: serde_json::to_string(&withdrawal_fee_memo).unwrap(),
        };
        payment_memos.push(payment_memo);
    }

    if let Some(claim_fee_transfer_index) = claim_fee_transfer_index {
        if claim_fee_transfer_index >= transfer_requests.len() as u32 {
            return Err(SyncError::FeeError(
                "claim_fee_transfer_index is out of range".to_string(),
            ));
        }
        let fee_transfer_req = &transfer_requests[claim_fee_transfer_index as usize];
        let fee = Fee {
            token_index: fee_transfer_req.token_index,
            amount: fee_transfer_req.amount,
        };
        let claim_fee_memo = ClaimFeeMemo { fee };
        let payment_memo = PaymentMemoEntry {
            transfer_index: claim_fee_transfer_index,
            topic: payment_memo_topic(CLAIM_FEE_MEMO),
            memo: serde_json::to_string(&claim_fee_memo).unwrap(),
        };
        payment_memos.push(payment_memo);
    }

    Ok(payment_memos)
}

/// quote fee and generate transfers for withdrawal and claim
pub async fn generate_withdrawal_transfers(
    withdrawal_server: &dyn WithdrawalServerClientInterface,
    withdrawal_contract: &WithdrawalContract,
    withdrawal_transfer_request: &TransferRequest,
    fee_token_index: u32,
    with_claim_fee: bool,
) -> Result<WithdrawalTransferRequests, SyncError> {
    let mut transfer_requests = if withdrawal_transfer_request.amount == U256::zero() {
        // if withdrawal_transfer.amount is zero, ignore withdrawal_transfer
        // and only generate fee transfers
        vec![]
    } else {
        vec![withdrawal_transfer_request.clone()]
    };

    let mut withdrawal_fee_transfer_index = None;
    let mut claim_fee_transfer_index = None;

    let (withdrawal_beneficiary, withdrawal_fee) = quote_withdrawal_fee(
        withdrawal_server,
        withdrawal_contract,
        withdrawal_transfer_request.token_index,
        fee_token_index,
    )
    .await?;
    if let Some(withdrawal_fee) = &withdrawal_fee {
        let withdrawal_fee_transfer = TransferRequest {
            token_index: withdrawal_fee.token_index,
            recipient: GenericRecipient::IntmaxAddress(withdrawal_beneficiary),
            amount: withdrawal_fee.amount,
            description: None,
        };
        withdrawal_fee_transfer_index = Some(transfer_requests.len() as u32);
        transfer_requests.push(withdrawal_fee_transfer);
    }
    if with_claim_fee {
        let (claim_beneficiary, claim_fee) =
            quote_claim_fee(withdrawal_server, fee_token_index).await?;
        if let Some(claim_fee) = claim_fee {
            let claim_fee_transfer = TransferRequest {
                token_index: claim_fee.token_index,
                recipient: GenericRecipient::IntmaxAddress(claim_beneficiary),
                amount: claim_fee.amount,
                description: None,
            };
            claim_fee_transfer_index = Some(transfer_requests.len() as u32);
            transfer_requests.push(claim_fee_transfer);
        }
    }
    Ok(WithdrawalTransferRequests {
        transfer_requests,
        withdrawal_fee_transfer_index,
        claim_fee_transfer_index,
    })
}

/// get unused payment memos
pub async fn get_unused_payments(
    store_vault_server: &dyn StoreVaultClientInterface,
    view_pair: ViewPair,
    fee_type: FeeType,
) -> Result<Vec<PaymentMemo>, SyncError> {
    let memo_name = match fee_type {
        FeeType::Withdrawal => WITHDRAWAL_FEE_MEMO,
        FeeType::Claim => CLAIM_FEE_MEMO,
    };
    let memos = get_all_payment_memos(store_vault_server, view_pair.view, memo_name).await?;
    let used_memos =
        get_all_payment_memos(store_vault_server, view_pair.view, USED_OR_INVALID_MEMO).await?;
    let unused_memos = memos
        .into_iter()
        .filter(|memo| {
            !used_memos
                .iter()
                .any(|used_memo| used_memo.meta.digest == memo.meta.digest)
        })
        .collect::<Vec<PaymentMemo>>();
    Ok(unused_memos)
}

/// consume payment memo
pub async fn consume_payment(
    store_vault_server: &dyn StoreVaultClientInterface,
    view_pair: ViewPair,
    payment_memo: &PaymentMemo,
    reason: &str,
) -> Result<(), SyncError> {
    let topic = payment_memo_topic(USED_OR_INVALID_MEMO);
    let memo = UsedOrInvalidMemo {
        reason: reason.to_string(),
    };
    let payment_memo = PaymentMemo {
        meta: payment_memo.meta.clone(),
        transfer_data: payment_memo.transfer_data.clone(),
        memo: serde_json::to_string(&memo).unwrap(),
    };
    let entry = SaveDataEntry {
        topic,
        pubkey: view_pair.view.to_public_key().0,
        data: payment_memo.encrypt(view_pair.view.to_public_key(), Some(view_pair.view))?,
    };
    store_vault_server
        .save_data_batch(view_pair.view, &[entry])
        .await?;
    Ok(())
}

/// select unused fees and validate them
pub async fn select_unused_fees(
    store_vault_server: &dyn StoreVaultClientInterface,
    validity_prover: &dyn ValidityProverClientInterface,
    view_pair: ViewPair,
    fee_beneficiary: U256,
    fee: Fee,
    fee_type: FeeType,
    tx_timeout: u64,
) -> Result<Vec<PaymentMemo>, SyncError> {
    let unused_fees = get_unused_payments(store_vault_server, view_pair, fee_type).await?;
    // Extract only those whose fee.token_index and recipient matches and sort by fee.amount
    let mut sorted_fee_memo = unused_fees
        .into_iter()
        .filter(|memo| {
            memo.transfer_data.transfer.token_index == fee.token_index
                && memo.transfer_data.transfer.recipient == fee_beneficiary.into()
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
            PublicKey(memo.transfer_data.transfer.recipient.to_pubkey().unwrap()),
            memo.meta.timestamp,
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
                    consume_payment(store_vault_server, view_pair, &memo, "tx is timeout").await?;
                }
                log::info!("fee: {} is not settled yet", memo.meta.digest);
                continue;
            }
            Err(e) => {
                log::warn!("invalid fee: {} reason: {}", memo.meta.digest, e,);
                consume_payment(store_vault_server, view_pair, &memo, &e.to_string()).await?;
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
