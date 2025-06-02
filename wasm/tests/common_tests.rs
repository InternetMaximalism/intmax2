#![cfg(target_arch = "wasm32")]

use intmax2_wasm_lib::js_types::common::{JsGenericAddress, JsTransfer};
use intmax2_zkp::common::{generic_address::GenericAddress, transfer::Transfer};
use wasm_bindgen::JsError;
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!();

#[wasm_bindgen_test]
fn test_js_generic_address_invalid_pubkey() {
    let js = JsGenericAddress {
        is_pubkey: true,
        data: "not_hex".to_string(),
    };
    let result: Result<GenericAddress, JsError> = (&js).try_into();
    assert!(result.is_err());
}

#[wasm_bindgen_test]
fn test_js_transfer_invalid_amount() {
    let js = JsTransfer {
        recipient: JsGenericAddress {
            is_pubkey: true,
            data: "0x02".to_string(),
        },
        token_index: 1,
        amount: "abc".to_string(), // invalid number
        salt: "0x1234".to_string(),
    };

    let result: Result<Transfer, JsError> = js.try_into();
    assert!(result.is_err());
}

#[wasm_bindgen_test]
fn test_to_withdrawal_eth_recipient() {
    let js = JsTransfer {
        recipient: JsGenericAddress {
            is_pubkey: false,
            data: "0x000000000000000000000000000000000000dead".to_string(),
        },
        token_index: 1,
        amount: "1000".to_string(),
        salt: "0x1234".to_string(),
    };

    let withdrawal = js.to_withdrawal();
    assert!(withdrawal.is_ok());

    let result = withdrawal.unwrap();
    assert_eq!(
        result.recipient,
        "0x000000000000000000000000000000000000dead"
    );
}

#[wasm_bindgen_test]
fn test_to_withdrawal_with_pubkey_should_fail() {
    let js = JsTransfer {
        recipient: JsGenericAddress {
            is_pubkey: true,
            data: "0x02".to_string(),
        },
        token_index: 1,
        amount: "1000".to_string(),
        salt: "0x1234".to_string(),
    };

    let withdrawal = js.to_withdrawal();
    assert!(withdrawal.is_err());
}
