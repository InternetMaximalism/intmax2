use intmax2_interfaces::api::block_builder::interface::Fee;
use intmax2_zkp::common::{claim::Claim, withdrawal::Withdrawal};
use serde::{Deserialize, Serialize};

use super::payment_memo::PaymentMemo;

pub type WithdrawalFeePaymentMemo = PaymentMemo<WithdrawalFeeMemo>;
pub type ClaimFeePaymentMemo = PaymentMemo<ClaimFeeMemo>;
pub type UsedOrInvalidPaymentMemo = PaymentMemo<UsedOrInvalidMemo>;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WithdrawalFeeMemo {
    pub withdrawal: Withdrawal,
    pub fee: Fee,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimFeeMemo {
    pub claim: Claim,
    pub fee: Fee,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsedOrInvalidMemo {
    pub reason: String,
}
