use intmax2_interfaces::api::validity_prover::interface::ValidityProverClientInterface;

use crate::external_api::utils::time::sleep_for;

use super::error::StrategyError;

const VALIDITY_PROVER_SYNC_SLEEP_TIME: u64 = 5;
const MAX_SYNC_TRIES: u32 = 5;
const MAX_PROOF_SYNC_TRIES: u32 = 40;

pub async fn wait_till_validity_prover_synced(
    validity_prover: &dyn ValidityProverClientInterface,
    wait_for_proof: bool,
    block_number: u32,
) -> Result<(), StrategyError> {
    let mut tries = 0;
    let mut synced_block_number = validity_prover.get_block_number().await?;
    while synced_block_number < block_number {
        if tries > MAX_SYNC_TRIES {
            return Err(StrategyError::ValidityProverIsNotSynced(format!(
                "expected block number {block_number} but got {synced_block_number} after {tries} tries"
            )));
        }
        tries += 1;
        log::warn!(
            "validity prover is not synced: target {block_number}, current {synced_block_number}"
        );

        sleep_for(VALIDITY_PROVER_SYNC_SLEEP_TIME).await;
        synced_block_number = validity_prover.get_block_number().await?;
    }
    if !wait_for_proof {
        return Ok(());
    }
    let mut validity_proof_block_number = validity_prover.get_validity_proof_block_number().await?;
    while validity_proof_block_number < block_number {
        if tries > MAX_PROOF_SYNC_TRIES {
            return Err(StrategyError::ValidityProverIsNotSynced(format!(
                "expected validity proof block number {block_number} but got {validity_proof_block_number} after {tries} tries"
            )));
        }
        tries += 1;
        log::warn!(
            "waiting for validity proof: target {block_number}, current {validity_proof_block_number}"
        );
        sleep_for(VALIDITY_PROVER_SYNC_SLEEP_TIME).await;
        validity_proof_block_number = validity_prover.get_validity_proof_block_number().await?;
    }

    Ok(())
}
