use alloy::primitives::B256;
use intmax2_client_sdk::external_api::contract::convert::convert_b256_to_bytes32;
use intmax2_zkp::{common::signature_content::key_set::KeySet, ethereum_types::bytes32::Bytes32};
use wasm_bindgen::JsError;

pub fn str_privkey_to_keyset(privkey: &str) -> Result<KeySet, JsError> {
    let privkey = parse_h256(privkey)?;
    let privkey = convert_b256_to_bytes32(privkey);
    Ok(KeySet::new(privkey.into()))
}

pub fn parse_h256(s: &str) -> Result<B256, JsError> {
    let x: B256 = s
        .parse()
        .map_err(|e| JsError::new(&format!("failed to parse b256 {e}")))?;
    Ok(x)
}

pub fn parse_bytes32(s: &str) -> Result<Bytes32, JsError> {
    let x: Bytes32 = s
        .parse()
        .map_err(|e| JsError::new(&format!("failed to parse bytes32 {e}")))?;
    Ok(x)
}
