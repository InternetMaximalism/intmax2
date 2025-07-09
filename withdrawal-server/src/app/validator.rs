use intmax2_client_sdk::external_api::contract::rollup_contract::RollupContract;
use intmax2_zkp::ethereum_types::{bytes32::Bytes32, u32limb_trait::U32LimbTrait};

use super::error::WithdrawalServerError;

#[async_trait::async_trait]
pub trait BlockHashValidator: Send + Sync {
    async fn validate_block_hash_existence(
        &self,
        contract: &RollupContract,
        block_number: u32,
        expected_hash: Bytes32,
    ) -> Result<(), WithdrawalServerError>;
}

pub struct RealBlockHashValidator;

#[async_trait::async_trait]
impl BlockHashValidator for RealBlockHashValidator {
    async fn validate_block_hash_existence(
        &self,
        contract: &RollupContract,
        block_number: u32,
        expected_hash: Bytes32,
    ) -> Result<(), WithdrawalServerError> {
        let onchain_hash = contract.get_block_hash(block_number).await?;
        if onchain_hash != expected_hash {
            return Err(WithdrawalServerError::InvalidBlockHash(format!(
                "Invalid block hash: expected {}, got {} at block number {}",
                expected_hash.to_hex(),
                onchain_hash.to_hex(),
                block_number
            )));
        }
        Ok(())
    }
}

pub struct MockBlockHashValidator;

#[async_trait::async_trait]
impl BlockHashValidator for MockBlockHashValidator {
    async fn validate_block_hash_existence(
        &self,
        _contract: &RollupContract,
        _block_number: u32,
        _expected_hash: Bytes32,
    ) -> Result<(), WithdrawalServerError> {
        Ok(())
    }
}
