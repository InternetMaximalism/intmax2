use ethers::types::H256;
use intmax2_zkp::{
    common::signature::key_set::KeySet,
    ethereum_types::{bytes32::Bytes32, u32limb_trait::U32LimbTrait},
};
use num_bigint::BigUint;
use wasm_bindgen::JsError;

pub fn h256_to_bytes32(h256: H256) -> Bytes32 {
    Bytes32::from_bytes_be(h256.as_bytes())
}

pub fn str_privkey_to_keyset(privkey: &str) -> Result<KeySet, JsError> {
    let privkey = parse_h256(privkey)?;
    Ok(h256_to_keyset(privkey))
}

fn h256_to_keyset(h256: H256) -> KeySet {
    KeySet::new(BigUint::from_bytes_be(h256.as_bytes()).into())
}

pub fn parse_h256(s: &str) -> Result<H256, JsError> {
    let x: H256 = s
        .parse()
        .map_err(|e| JsError::new(&format!("failed to parse h256 {}", e)))?;
    Ok(x)
}
