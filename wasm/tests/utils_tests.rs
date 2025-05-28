#![cfg(target_arch = "wasm32")]

use intmax2_wasm_lib::js_types::utils::{
    parse_address, parse_bytes32, parse_poseidon_hashout, parse_salt, parse_u256,
};
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!();

#[wasm_bindgen_test]
fn test_parse_u256_valid() {
    let result = parse_u256("123456789012345678901234567890");
    assert!(result.is_ok());
}

#[wasm_bindgen_test]
fn test_parse_u256_invalid_non_numeric() {
    let result = parse_u256("notanumber");
    assert!(result.is_err());
}

#[wasm_bindgen_test]
fn test_parse_bytes32_valid() {
    let result =
        parse_bytes32("0x0000000000000000000000000000000000000000000000000000000000000001");
    assert!(result.is_ok());
}

#[wasm_bindgen_test]
fn test_parse_bytes32_invalid_hex_chars() {
    let result =
        parse_bytes32("0xzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz");
    assert!(result.is_err());
}

#[wasm_bindgen_test]
fn test_parse_bytes32_short_length() {
    let result = parse_bytes32("0x1234");
    assert!(result.is_ok());
}

#[wasm_bindgen_test]
fn test_parse_address_valid() {
    // Used rollup contract address
    let result = parse_address("0xe7f1725e7734ce288f8367e1bb143e90bb3f0512");
    assert!(result.is_ok());
}

#[wasm_bindgen_test]
fn test_parse_address_invalid_hex() {
    let result = parse_address("0xINVALIDHEXADDRESS");
    assert!(result.is_err());
}

#[wasm_bindgen_test]
fn test_parse_address_short_length() {
    let result = parse_address("0x1234");
    assert!(result.is_ok());
}

#[wasm_bindgen_test]
fn test_parse_poseidon_hashout_valid() {
    let result = parse_poseidon_hashout(
        "0x0000000000000000000000000000000000000000000000000000000000000001",
    );
    assert!(result.is_ok());
}

#[wasm_bindgen_test]
fn test_parse_poseidon_hashout_invalid() {
    let result = parse_poseidon_hashout("0xNOTVALID");
    assert!(result.is_err());
}

#[wasm_bindgen_test]
fn test_parse_salt_valid() {
    let result = parse_salt("0x1111111111111111111111111111111111111111111111111111111111111111");
    assert!(result.is_ok());
}

#[wasm_bindgen_test]
fn test_parse_salt_invalid() {
    let result = parse_salt("zz");
    assert!(result.is_err());
}
