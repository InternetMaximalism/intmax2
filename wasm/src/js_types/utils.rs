use std::str::FromStr;

use intmax2_zkp::{
    common::salt::Salt,
    ethereum_types::{address::Address, bytes32::Bytes32, u256::U256, u32limb_trait::U32LimbTrait},
    utils::poseidon_hash_out::PoseidonHashOut,
};
use num_bigint::BigUint;
use wasm_bindgen::JsError;

fn js_err<T, E: ToString>(result: Result<T, E>, msg: &'static str) -> Result<T, JsError> {
    result.map_err(|_| JsError::new(msg))
}

pub fn parse_u256(input: &str) -> Result<U256, JsError> {
    let big_uint = js_err(
        BigUint::from_str(input),
        "Failed to parse as BigUint. Expected decimal string",
    )?;
    js_err(big_uint.try_into(), "Failed to cast to U256")
}

pub fn parse_bytes32(input: &str) -> Result<Bytes32, JsError> {
    js_err(
        Bytes32::from_hex(input),
        "Failed to parse as Bytes32. Expected 0x-prefixed hex string",
    )
}

pub fn parse_address(input: &str) -> Result<Address, JsError> {
    js_err(
        Address::from_hex(input),
        "Failed to parse as Address. Expected 0x-prefixed hex Ethereum address",
    )
}

pub fn parse_poseidon_hashout(input: &str) -> Result<PoseidonHashOut, JsError> {
    Ok(parse_bytes32(input)?.reduce_to_hash_out())
}

pub fn parse_salt(input: &str) -> Result<Salt, JsError> {
    Ok(Salt(parse_poseidon_hashout(input)?))
}
