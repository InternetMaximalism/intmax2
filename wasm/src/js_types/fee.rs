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
                .map_err(|e| JsError::new(&format!("Invalid beneficiary address: {e}")))?,
            fee: js_fee_quote.fee.map(JsFee::try_into).transpose()?,
            collateral_fee: js_fee_quote
                .collateral_fee
                .map(JsFee::try_into)
                .transpose()?,
            block_builder_address: Address::from_hex(&js_fee_quote.block_builder_address)
                .map_err(|e| JsError::new(&format!("Invalid block builder address: {e}")))?,
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

fn convert_fees(fees: Option<Vec<Fee>>) -> Option<Vec<JsFee>> {
    fees.map(|f| f.into_iter().map(JsFee::from).collect())
}

impl From<BlockBuilderFeeInfo> for JsFeeInfo {
    fn from(fee_info: BlockBuilderFeeInfo) -> Self {
        Self {
            beneficiary: fee_info.beneficiary.map(|b| b.to_hex()),
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

#[cfg(test)]
mod fee_tests {
    use std::str::FromStr;

    use intmax2_client_sdk::client::{client::FeeQuote, fee_payment::WithdrawalTransfers};
    use intmax2_interfaces::api::block_builder::interface::{BlockBuilderFeeInfo, Fee};
    use intmax2_zkp::ethereum_types::{address::Address, u256::U256};

    use crate::js_types::{
        common::{JsGenericAddress, JsTransfer},
        fee::{JsFee, JsFeeInfo, JsFeeQuote, JsWithdrawalTransfers},
    };

    fn fee(amount: &str, token_index: u32) -> Fee {
        Fee {
            amount: U256::from_str(amount).unwrap(),
            token_index,
        }
    }

    fn dummy_js_transfer() -> JsTransfer {
        JsTransfer {
            recipient: JsGenericAddress {
                is_pubkey: false,
                data: "0x0000000000000000000000000000000000000000".to_string(),
            },
            token_index: 0,
            amount: "0".to_string(),
            salt: "0x00".to_string(),
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
            beneficiary: Some(
                U256::from_str("0x1111111111111111111111111111111111111111").unwrap(),
            ),
            fee: Some(fee("100", 1)),
            collateral_fee: Some(fee("200", 2)),
        };

        let js_quote = JsFeeQuote::from(quote);

        assert_eq!(
            js_quote.beneficiary,
            Some("0x0000000000000000000000001111111111111111111111111111111111111111".to_string())
        );
        assert_eq!(js_quote.fee.as_ref().unwrap().amount, "100");
        assert_eq!(js_quote.fee.as_ref().unwrap().token_index, 1);
        assert_eq!(js_quote.collateral_fee.as_ref().unwrap().amount, "200");
        assert_eq!(js_quote.collateral_fee.as_ref().unwrap().token_index, 2);
    }

    #[test]
    fn test_blockbuilderfeeinfo_to_jsfeeinfo() {
        let info = BlockBuilderFeeInfo {
            beneficiary: Some(
                U256::from_str("0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").unwrap(),
            ),
            registration_fee: Some(vec![fee("10", 0), fee("20", 1)]),
            non_registration_fee: None,
            registration_collateral_fee: Some(vec![fee("30", 2)]),
            non_registration_collateral_fee: None,
            block_builder_address: Address::default(),
        };

        let js_info = JsFeeInfo::from(info);

        assert_eq!(
            js_info.beneficiary,
            Some("0x000000000000000000000000aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string())
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
        let js_transfer = dummy_js_transfer();
        let js_transfers = JsWithdrawalTransfers {
            transfers: vec![js_transfer],
            withdrawal_fee_transfer_index: Some(0),
            claim_fee_transfer_index: Some(1),
        };

        let wt: WithdrawalTransfers = js_transfers.clone().try_into().unwrap();
        assert_eq!(wt.transfers.len(), 1);
        assert_eq!(wt.withdrawal_fee_transfer_index, Some(0));
        assert_eq!(wt.claim_fee_transfer_index, Some(1));

        let js_back = JsWithdrawalTransfers::from(wt);
        assert_eq!(js_back.transfers.len(), 1);
        assert_eq!(js_back.withdrawal_fee_transfer_index, Some(0));
        assert_eq!(js_back.claim_fee_transfer_index, Some(1));
    }
}
