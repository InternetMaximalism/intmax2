use ethers::types::{Address, U256};
use intmax2_zkp::ethereum_types::u32limb_trait::U32LimbTrait as _;

use common::env::EnvType;

use crate::env_var::EnvVar;

use super::error::CliError;

pub fn convert_u256(input: intmax2_zkp::ethereum_types::u256::U256) -> U256 {
    U256::from_big_endian(&input.to_bytes_be())
}

pub fn convert_address(input: Address) -> intmax2_zkp::ethereum_types::address::Address {
    intmax2_zkp::ethereum_types::address::Address::from_bytes_be(&input.to_fixed_bytes())
}

pub fn load_env() -> Result<EnvVar, CliError> {
    let env = envy::from_env::<EnvVar>()?;
    Ok(env)
}

pub fn is_local() -> Result<bool, CliError> {
    Ok(load_env()?.env == EnvType::Local)
}
