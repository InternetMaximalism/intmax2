use intmax2_client_sdk::client::client::FeeQuote;
use intmax2_interfaces::api::block_builder::interface::{Fee, FeeInfo};
use intmax2_zkp::ethereum_types::u32limb_trait::U32LimbTrait as _;
use wasm_bindgen::{prelude::wasm_bindgen, JsError};

use super::utils::parse_u256;

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

impl From<FeeInfo> for JsFeeInfo {
    fn from(fee_info: FeeInfo) -> Self {
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
