#![cfg(target_arch = "wasm32")]

use intmax2_wasm_lib::{generate_intmax_account_from_eth_key, get_deposit_hash};
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!();

#[wasm_bindgen_test]
async fn test_generate_account_from_eth_key() {
    let network = "mainnet";
    let eth_key = "0x0000000000000000000000000000000000000000000000000000000000000001";
    let is_legacy = false;
    let result = generate_intmax_account_from_eth_key(network, eth_key, is_legacy).await;
    assert!(result.is_ok(), "Account generation failed");

    let account = result.unwrap();
    assert!(!account.address.is_empty(), "Empty address");
    assert!(!account.view_pair.is_empty(), "Empty view pair");
}

#[wasm_bindgen_test]
fn test_get_deposit_hash_basic() {
    let depositor = "0x0000000000000000000000000000000000000000";
    let salt_hash = "0x0000000000000000000000000000000000000000000000000000000000000000";
    let token_index = 0;
    let amount = "1000";
    let is_eligible = true;

    let result = get_deposit_hash(depositor, salt_hash, token_index, amount, is_eligible);
    assert!(result.is_ok(), "Failed to compute deposit hash");

    let hash = result.unwrap();
    assert_eq!(hash.len(), 66, "Invalid hash length");
}
