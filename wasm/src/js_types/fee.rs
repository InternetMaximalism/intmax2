use intmax2_client_sdk::client::{
    client::{FeeQuote, TransferFeeQuote},
    fee_payment::WithdrawalTransfers,
};
use intmax2_interfaces::api::block_builder::interface::{BlockBuilderFeeInfo, Fee};
use intmax2_zkp::ethereum_types::{address::Address, u256::U256, u32limb_trait::U32LimbTrait as _};
use wasm_bindgen::{prelude::wasm_bindgen, JsError};

use super::{common::JsTransfer, utils::parse_u256};

#[derive(Debug, Clone)]
#[wasm_bindgen(getter_with_clone)]
pub struct JsFee {
    pub amount: String, // 10 base string
    pub token_index: u32,
}

#[wasm_bindgen]
impl JsFee {
    #[wasm_bindgen(constructor)]
    pub fn new(amount: String, token_index: u32) -> Self {
        Self {
            amount,
            token_index,
        }
    }
}

impl TryFrom<JsFee> for Fee {
    type Error = JsError;

    fn try_from(js_fee: JsFee) -> Result<Self, JsError> {
        let amount = parse_u256(&js_fee.amount)?;
        Ok(Fee {
            amount,
            token_index: js_fee.token_index,
        })
    }
}

impl From<Fee> for JsFee {
    fn from(fee: Fee) -> Self {
        Self {
            amount: fee.amount.to_string(),
            token_index: fee.token_index,
        }
    }
}

#[derive(Debug, Clone)]
#[wasm_bindgen(getter_with_clone)]
pub struct JsTransferFeeQuote {
    pub beneficiary: Option<String>,
    pub fee: Option<JsFee>,
    pub collateral_fee: Option<JsFee>,
    pub block_builder_address: String,
}

impl From<TransferFeeQuote> for JsTransferFeeQuote {
    fn from(fee_quote: TransferFeeQuote) -> Self {
        Self {
            beneficiary: fee_quote.beneficiary.map(|b| b.to_hex()),
            fee: fee_quote.fee.map(JsFee::from),
            collateral_fee: fee_quote.collateral_fee.map(JsFee::from),
            block_builder_address: fee_quote.block_builder_address.to_hex(),
        }
    }
}

impl TryFrom<JsTransferFeeQuote> for TransferFeeQuote {
    type Error = JsError;

    fn try_from(js_fee_quote: JsTransferFeeQuote) -> Result<Self, JsError> {
        Ok(TransferFeeQuote {
            beneficiary: js_fee_quote
                .beneficiary
                .map(|b| U256::from_hex(&b))
                .transpose()
                .map_err(|e| JsError::new(&format!("Invalid beneficiary address: {}", e)))?,
            fee: js_fee_quote.fee.map(JsFee::try_into).transpose()?,
            collateral_fee: js_fee_quote
                .collateral_fee
                .map(JsFee::try_into)
                .transpose()?,
            block_builder_address: Address::from_hex(&js_fee_quote.block_builder_address)
                .map_err(|e| JsError::new(&format!("Invalid block builder address: {}", e)))?,
        })
    }
}

#[derive(Debug, Clone)]
#[wasm_bindgen(getter_with_clone)]
pub struct JsFeeQuote {
    pub beneficiary: Option<String>,
    pub fee: Option<JsFee>,
    pub collateral_fee: Option<JsFee>,
}

impl From<FeeQuote> for JsFeeQuote {
    fn from(fee_quote: FeeQuote) -> Self {
        Self {
            beneficiary: fee_quote.beneficiary.map(|b| b.to_hex()),
            fee: fee_quote.fee.map(JsFee::from),
            collateral_fee: fee_quote.collateral_fee.map(JsFee::from),
        }
    }
}

#[derive(Debug, Clone)]
#[wasm_bindgen(getter_with_clone)]
pub struct JsFeeInfo {
    pub beneficiary: Option<String>,
    pub registration_fee: Option<Vec<JsFee>>,
    pub non_registration_fee: Option<Vec<JsFee>>,
    pub registration_collateral_fee: Option<Vec<JsFee>>,
    pub non_registration_collateral_fee: Option<Vec<JsFee>>,
}

impl From<BlockBuilderFeeInfo> for JsFeeInfo {
    fn from(fee_info: BlockBuilderFeeInfo) -> Self {
        Self {
            beneficiary: fee_info.beneficiary.map(|b| b.to_hex()),
            registration_fee: fee_info
                .registration_fee
                .map(|fees| fees.into_iter().map(JsFee::from).collect()),
            non_registration_fee: fee_info
                .non_registration_fee
                .map(|fees| fees.into_iter().map(JsFee::from).collect()),
            registration_collateral_fee: fee_info
                .registration_collateral_fee
                .map(|fees| fees.into_iter().map(JsFee::from).collect()),
            non_registration_collateral_fee: fee_info
                .non_registration_collateral_fee
                .map(|fees| fees.into_iter().map(JsFee::from).collect()),
        }
    }
}

#[derive(Debug, Clone)]
#[wasm_bindgen(getter_with_clone)]
pub struct JsWithdrawalTransfers {
    pub transfers: Vec<JsTransfer>,
    pub withdrawal_fee_transfer_index: Option<u32>,
    pub claim_fee_transfer_index: Option<u32>,
}

impl From<WithdrawalTransfers> for JsWithdrawalTransfers {
    fn from(withdrawal_transfers: WithdrawalTransfers) -> Self {
        Self {
            transfers: withdrawal_transfers
                .transfers
                .into_iter()
                .map(JsTransfer::from)
                .collect(),
            withdrawal_fee_transfer_index: withdrawal_transfers.withdrawal_fee_transfer_index,
            claim_fee_transfer_index: withdrawal_transfers.claim_fee_transfer_index,
        }
    }
}

impl TryFrom<JsWithdrawalTransfers> for WithdrawalTransfers {
    type Error = JsError;

    fn try_from(js_withdrawal_transfers: JsWithdrawalTransfers) -> Result<Self, JsError> {
        Ok(WithdrawalTransfers {
            transfers: js_withdrawal_transfers
                .transfers
                .into_iter()
                .map(|t| t.try_into())
                .collect::<Result<_, _>>()?,
            withdrawal_fee_transfer_index: js_withdrawal_transfers.withdrawal_fee_transfer_index,
            claim_fee_transfer_index: js_withdrawal_transfers.claim_fee_transfer_index,
        })
    }
}
