use ethers::types::H256;
use intmax2_zkp::{
    common::signature_content::key_set::KeySet,
    ethereum_types::{bytes32::Bytes32, u256::U256, u32limb_trait::U32LimbTrait},
};
use num_bigint::BigUint;
use wasm_bindgen::JsError;

pub fn h256_to_bytes32(h256: H256) -> Bytes32 {
    Bytes32::from_bytes_be(h256.as_bytes()).unwrap()
}

pub fn str_privkey_to_keyset(privkey: &str) -> Result<KeySet, JsError> {
    let privkey = parse_h256(privkey)?;
    Ok(h256_to_keyset(privkey))
}

fn h256_to_keyset(h256: H256) -> KeySet {
    let key: U256 = BigUint::from_bytes_be(h256.as_bytes()).try_into().unwrap();
    KeySet::new(key)
}

pub fn parse_h256(s: &str) -> Result<H256, JsError> {
    let x: H256 = s
        .parse()
        .map_err(|e| JsError::new(&format!("failed to parse h256 {}", e)))?;
    Ok(x)
}

pub fn parse_h256_as_u256(s: &str) -> Result<U256, JsError> {
    let x = parse_h256(s)?;
    Ok(h256_to_bytes32(x).into())
}
