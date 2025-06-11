use alloy::primitives::B256;
use intmax2_interfaces::utils::key::{KeyPair, ViewPair};
use intmax2_zkp::{common::signature_content::key_set::KeySet, ethereum_types::bytes32::Bytes32};
use wasm_bindgen::JsError;

pub fn str_to_view_pair(view_pair: &str) -> Result<ViewPair, JsError> {
    let view_pair = view_pair
        .parse()
        .map_err(|e| JsError::new(&format!("failed to parse view pair {e}")))?;
    Ok(view_pair)
}

pub fn str_to_key_pair(key_pair: &str) -> Result<KeyPair, JsError> {
    let key_pair = key_pair
        .parse()
        .map_err(|e| JsError::new(&format!("failed to parse key pair {e}")))?;
    Ok(key_pair)
}

pub fn str_to_keyset(private_key: &str) -> Result<KeySet, JsError> {
    let private_key: Bytes32 = private_key
        .parse()
        .map_err(|e| JsError::new(&format!("failed to parse private key {e}")))?;
    let key_set = KeySet::new(private_key.into());
    Ok(key_set)
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
