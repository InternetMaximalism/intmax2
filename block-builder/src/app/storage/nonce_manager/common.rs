use alloy::primitives::Address;
use intmax2_client_sdk::external_api::contract::{
    error::BlockchainError, rollup_contract::RollupContract,
};

const IGNORED_NONCE: u32 = 0;
const DEFAULT_NONCE: u32 = 1;

pub async fn get_onchain_next_nonce(
    rollup: &RollupContract,
    is_registration: bool,
    block_builder_address: Address,
) -> Result<u32, BlockchainError> {
    let mut onchain_next_nonce = rollup
        .get_block_builder_nonce(is_registration, block_builder_address)
        .await?;
    if onchain_next_nonce == IGNORED_NONCE {
        // If the on-chain nonce is 0, we set it to 1 to avoid conflicts with the sentinel value (ignored nonce).
        onchain_next_nonce = DEFAULT_NONCE;
    }
    Ok(onchain_next_nonce)
}
