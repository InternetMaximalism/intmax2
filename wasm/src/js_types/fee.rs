use intmax2_client_sdk::client::{
    fee_payment::WithdrawalTransferRequests,
    types::{FeeQuote, TransferFeeQuote},
};
use intmax2_interfaces::{api::block_builder::interface::BlockBuilderFeeInfo, utils::fee::Fee};
use intmax2_zkp::ethereum_types::{address::Address, u32limb_trait::U32LimbTrait as _};
use wasm_bindgen::{prelude::wasm_bindgen, JsError};

use crate::js_types::client::JsTransferRequest;

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
pub struct JsTransferFeeQuote {
    pub beneficiary: String,
    pub fee: Option<JsFee>,
    pub collateral_fee: Option<JsFee>,
    pub block_builder_address: String,
    pub is_registration_block: bool,
}

impl From<TransferFeeQuote> for JsTransferFeeQuote {
    fn from(fee_quote: TransferFeeQuote) -> Self {
        Self {
            beneficiary: fee_quote.beneficiary.to_string(),
            fee: fee_quote.fee.map(JsFee::from),
            collateral_fee: fee_quote.collateral_fee.map(JsFee::from),
            block_builder_address: fee_quote.block_builder_address.to_hex(),
            is_registration_block: fee_quote.is_registration_block,
        }
    }
}

impl TryFrom<JsTransferFeeQuote> for TransferFeeQuote {
    type Error = JsError;

    fn try_from(js_fee_quote: JsTransferFeeQuote) -> Result<Self, JsError> {
        Ok(TransferFeeQuote {
            beneficiary: js_fee_quote
                .beneficiary
                .parse()
                .map_err(|e| JsError::new(&format!("Invalid beneficiary address: {e}")))?,
            fee: js_fee_quote.fee.map(JsFee::try_into).transpose()?,
            collateral_fee: js_fee_quote
                .collateral_fee
                .map(JsFee::try_into)
                .transpose()?,
            block_builder_address: Address::from_hex(&js_fee_quote.block_builder_address)
                .map_err(|e| JsError::new(&format!("Invalid block builder address: {e}")))?,
            is_registration_block: js_fee_quote.is_registration_block,
        })
    }
}

#[derive(Debug, Clone)]
#[wasm_bindgen(getter_with_clone)]
pub struct JsFeeQuote {
    pub beneficiary: String,
    pub fee: Option<JsFee>,
    pub collateral_fee: Option<JsFee>,
}

impl From<FeeQuote> for JsFeeQuote {
    fn from(fee_quote: FeeQuote) -> Self {
        Self {
            beneficiary: fee_quote.beneficiary.to_string(),
            fee: fee_quote.fee.map(JsFee::from),
            collateral_fee: fee_quote.collateral_fee.map(JsFee::from),
        }
    }
}

#[derive(Debug, Clone)]
#[wasm_bindgen(getter_with_clone)]
pub struct JsFeeInfo {
    pub beneficiary: String,
    pub registration_fee: Option<Vec<JsFee>>,
    pub non_registration_fee: Option<Vec<JsFee>>,
    pub registration_collateral_fee: Option<Vec<JsFee>>,
    pub non_registration_collateral_fee: Option<Vec<JsFee>>,
}

fn convert_fees(fees: Option<Vec<Fee>>) -> Option<Vec<JsFee>> {
    fees.map(|f| f.into_iter().map(JsFee::from).collect())
}

impl From<BlockBuilderFeeInfo> for JsFeeInfo {
    fn from(fee_info: BlockBuilderFeeInfo) -> Self {
        Self {
            beneficiary: fee_info.beneficiary.to_string(),
            registration_fee: convert_fees(fee_info.registration_fee),
            non_registration_fee: convert_fees(fee_info.non_registration_fee),
            registration_collateral_fee: convert_fees(fee_info.registration_collateral_fee),
            non_registration_collateral_fee: convert_fees(fee_info.non_registration_collateral_fee),
        }
    }
}

#[derive(Debug, Clone)]
#[wasm_bindgen(getter_with_clone)]
pub struct JsWithdrawalTransfers {
    pub transfer_requests: Vec<JsTransferRequest>,
    pub withdrawal_fee_transfer_index: Option<u32>,
    pub claim_fee_transfer_index: Option<u32>,
}

impl From<WithdrawalTransferRequests> for JsWithdrawalTransfers {
    fn from(withdrawal_transfers: WithdrawalTransferRequests) -> Self {
        Self {
            transfer_requests: withdrawal_transfers
                .transfer_requests
                .into_iter()
                .map(JsTransferRequest::from)
                .collect(),
            withdrawal_fee_transfer_index: withdrawal_transfers.withdrawal_fee_transfer_index,
            claim_fee_transfer_index: withdrawal_transfers.claim_fee_transfer_index,
        }
    }
}

impl TryFrom<JsWithdrawalTransfers> for WithdrawalTransferRequests {
    type Error = JsError;

    fn try_from(js_withdrawal_transfers: JsWithdrawalTransfers) -> Result<Self, JsError> {
        Ok(WithdrawalTransferRequests {
            transfer_requests: js_withdrawal_transfers
                .transfer_requests
                .into_iter()
                .map(|t| t.try_into())
                .collect::<Result<_, _>>()?,
            withdrawal_fee_transfer_index: js_withdrawal_transfers.withdrawal_fee_transfer_index,
            claim_fee_transfer_index: js_withdrawal_transfers.claim_fee_transfer_index,
        })
    }
}

#[cfg(test)]
mod fee_tests {
    use std::str::FromStr;

    use intmax2_client_sdk::client::{fee_payment::WithdrawalTransferRequests, types::FeeQuote};
    use intmax2_interfaces::{api::block_builder::interface::BlockBuilderFeeInfo, utils::fee::Fee};
    use intmax2_zkp::ethereum_types::{address::Address, u256::U256};

    use crate::js_types::{
        client::JsTransferRequest,
        fee::{JsFee, JsFeeInfo, JsFeeQuote, JsWithdrawalTransfers},
    };

    fn fee(amount: &str, token_index: u32) -> Fee {
        Fee {
            amount: U256::from_str(amount).unwrap(),
            token_index,
        }
    }

    fn dummy_js_transfer_request() -> JsTransferRequest {
        JsTransferRequest {
            recipient: "0x0000000000000000000000000000000000000000".to_string(),
            token_index: 0,
            amount: "0".to_string(),
            description: None,
        }
    }

    #[test]
    fn test_jsfee_new_and_conversion() {
        let js_fee = JsFee::new("12345".to_string(), 2);
        let fee: Fee = js_fee.clone().try_into().unwrap();
        assert_eq!(fee.amount, U256::from(12345u32));
        assert_eq!(fee.token_index, 2);

        let js_fee_back = JsFee::from(fee);
        assert_eq!(js_fee.amount, js_fee_back.amount);
        assert_eq!(js_fee.token_index, js_fee_back.token_index);
    }

    #[test]
    fn test_feequote_to_jsfeequote() {
        let quote = FeeQuote {
            beneficiary: "X7p9827sHEvXEePM2bLy8UQvB3Gx6FXZw65u3YFtaZE6Sm4mVYrZ7s6dAu1Sbg1Kg2b4SPHZqadsw4h3vkjdDG37A5TzZq8".parse().unwrap(),
            fee: Some(fee("100", 1)),
            collateral_fee: Some(fee("200", 2)),
        };

        let js_quote = JsFeeQuote::from(quote);

        assert_eq!(
            js_quote.beneficiary,
           "X7p9827sHEvXEePM2bLy8UQvB3Gx6FXZw65u3YFtaZE6Sm4mVYrZ7s6dAu1Sbg1Kg2b4SPHZqadsw4h3vkjdDG37A5TzZq8",
        );
        assert_eq!(js_quote.fee.as_ref().unwrap().amount, "100");
        assert_eq!(js_quote.fee.as_ref().unwrap().token_index, 1);
        assert_eq!(js_quote.collateral_fee.as_ref().unwrap().amount, "200");
        assert_eq!(js_quote.collateral_fee.as_ref().unwrap().token_index, 2);
    }

    #[test]
    fn test_blockbuilderfeeinfo_to_jsfeeinfo() {
        let info = BlockBuilderFeeInfo {
            version: "0.1.0".to_string(),
            beneficiary: "X7p9827sHEvXEePM2bLy8UQvB3Gx6FXZw65u3YFtaZE6Sm4mVYrZ7s6dAu1Sbg1Kg2b4SPHZqadsw4h3vkjdDG37A5TzZq8".parse().unwrap(),
            registration_fee: Some(vec![fee("10", 0), fee("20", 1)]),
            non_registration_fee: None,
            registration_collateral_fee: Some(vec![fee("30", 2)]),
            non_registration_collateral_fee: None,
            block_builder_address: Address::default(),
        };

        let js_info = JsFeeInfo::from(info);

        assert_eq!(
            js_info.beneficiary,
          "X7p9827sHEvXEePM2bLy8UQvB3Gx6FXZw65u3YFtaZE6Sm4mVYrZ7s6dAu1Sbg1Kg2b4SPHZqadsw4h3vkjdDG37A5TzZq8",
        );

        let reg_fees = js_info.registration_fee.unwrap();
        assert_eq!(reg_fees.len(), 2);
        assert_eq!(reg_fees[0].amount, "10");
        assert_eq!(reg_fees[1].token_index, 1);

        let reg_col_fees = js_info.registration_collateral_fee.unwrap();
        assert_eq!(reg_col_fees[0].amount, "30");
        assert_eq!(reg_col_fees[0].token_index, 2);
    }

    #[test]
    fn test_withdrawaltransfers_conversion_roundtrip() {
        let js_transfer_request = dummy_js_transfer_request();
        let js_transfers = JsWithdrawalTransfers {
            transfer_requests: vec![js_transfer_request],
            withdrawal_fee_transfer_index: Some(0),
            claim_fee_transfer_index: Some(1),
        };

        let wt: WithdrawalTransferRequests = js_transfers.clone().try_into().unwrap();
        assert_eq!(wt.transfer_requests.len(), 1);
        assert_eq!(wt.withdrawal_fee_transfer_index, Some(0));
        assert_eq!(wt.claim_fee_transfer_index, Some(1));

        let js_back = JsWithdrawalTransfers::from(wt);
        assert_eq!(js_back.transfer_requests.len(), 1);
        assert_eq!(js_back.withdrawal_fee_transfer_index, Some(0));
        assert_eq!(js_back.claim_fee_transfer_index, Some(1));
    }
}
