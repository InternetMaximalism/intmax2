use ethers::types::{Address as EthersAddress, H256, U256 as EthersU256};
use intmax2_zkp::ethereum_types::{
    address::Address as IntmaxAddress, bytes32::Bytes32, u256::U256 as IntmaxU256,
    u32limb_trait::U32LimbTrait,
};

pub fn convert_u256_to_ethers(input: IntmaxU256) -> EthersU256 {
    EthersU256::from_big_endian(&input.to_bytes_be())
}

pub fn convert_u256_to_intmax(input: EthersU256) -> IntmaxU256 {
    let mut bytes = [0u8; 32];
    input.to_big_endian(&mut bytes);
    IntmaxU256::from_bytes_be(&bytes).unwrap()
}

pub fn convert_address_to_ethers(input: IntmaxAddress) -> EthersAddress {
    EthersAddress::from_slice(&input.to_bytes_be())
}

pub fn convert_address_to_intmax(input: EthersAddress) -> IntmaxAddress {
    IntmaxAddress::from_bytes_be(&input.to_fixed_bytes()).unwrap()
}

pub fn convert_h256_to_bytes32(input: H256) -> Bytes32 {
    Bytes32::from_bytes_be(&input.to_fixed_bytes()).unwrap()
}

pub fn convert_bytes32_to_h256(input: Bytes32) -> H256 {
    H256::from_slice(&input.to_bytes_be())
}
