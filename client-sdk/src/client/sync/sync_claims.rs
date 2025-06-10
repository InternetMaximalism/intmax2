use intmax2_interfaces::{
    api::withdrawal_server::interface::{ClaimFeeInfo, FeeResult},
    utils::key::ViewPair,
};
use intmax2_zkp::{
    common::witness::{
        claim_witness::ClaimWitness,
        deposit_time_witness::{DepositTimePublicWitness, DepositTimeWitness},
    },
    ethereum_types::address::Address,
};

use crate::client::{
    client::Client,
    fee_payment::{consume_payment, select_unused_fees, FeeType},
    strategy::{
        mining::MiningStatus, strategy::determine_claims, utils::wait_till_validity_prover_synced,
    },
};

use super::{error::SyncError, utils::quote_withdrawal_claim_fee};

impl Client {
    /// Sync the client's withdrawals and relays to the withdrawal server
    pub async fn sync_claims(
        &self,
        view_pair: ViewPair,
        recipient: Address,
        fee_info: &ClaimFeeInfo,
        fee_token_index: u32,
    ) -> Result<(), SyncError> {
        let fee = quote_withdrawal_claim_fee(Some(fee_token_index), fee_info.fee.clone())?;
        let minings = determine_claims(
            self.store_vault_server.as_ref(),
            self.validity_prover.as_ref(),
            self.withdrawal_server.as_ref(),
            &self.rollup_contract,
            &self.liquidity_contract,
            self.config.is_faster_mining,
            view_pair,
            self.config.tx_timeout,
            self.config.deposit_timeout,
        )
        .await?;
        if minings.is_empty() {
            log::info!("No claimable mining found");
            return Ok(());
        }
        for mining in minings {
            log::info!("sync_claim: {:?}", mining.meta);

            let claim_block_number = match mining.status {
                MiningStatus::Claimable(block_number) => block_number,
                _ => {
                    // this should never happen because we only claim claimable minings
                    panic!("mining status is not claimable");
                }
            };

            wait_till_validity_prover_synced(
                self.validity_prover.as_ref(),
                true,
                claim_block_number,
            )
            .await?;

            // collect witnesses
            let block = mining.block.unwrap(); // safe to unwrap because it's already settled
            let deposit_block_number = block.block_number;
            let update_witness = self
                .validity_prover
                .get_update_witness(
                    view_pair.spend.0,
                    claim_block_number,
                    deposit_block_number,
                    false,
                )
                .await?;
            let last_block_number = update_witness.account_membership_proof.get_value() as u32;
            if deposit_block_number <= last_block_number {
                return Err(SyncError::InternalError(format!(
                    "deposit block number {deposit_block_number} is less than last block number {last_block_number}"
                )));
            }
            let deposit_hash = mining.deposit_data.deposit_hash().unwrap();
            let deposit_info = self
                .validity_prover
                .get_deposit_info(mining.deposit_data.pubkey_salt_hash)
                .await?
                .ok_or(SyncError::DepositInfoNotFound(deposit_hash))?;
            let deposit_index = deposit_info
                .deposit_index
                .ok_or(SyncError::DepositIsNotSettled(deposit_info.deposit_hash))?;

            let prev_block = self
                .validity_prover
                .get_validity_witness(deposit_block_number - 1)
                .await?
                .block_witness
                .block;
            let prev_deposit_merkle_proof = self
                .validity_prover
                .get_deposit_merkle_proof(deposit_block_number - 1, deposit_index)
                .await?;
            let deposit_merkle_proof = self
                .validity_prover
                .get_deposit_merkle_proof(deposit_block_number, deposit_index)
                .await?;
            let public_witness = DepositTimePublicWitness {
                prev_block,
                block,
                prev_deposit_merkle_proof,
                deposit_merkle_proof,
            };
            let deposit_time_witness = DepositTimeWitness {
                public_witness,
                deposit_index,
                deposit: mining.deposit_data.deposit().unwrap(),
                deposit_salt: mining.deposit_data.deposit_salt,
                pubkey: view_pair.spend.0,
            };
            let claim_witness = ClaimWitness {
                recipient,
                deposit_time_witness,
                update_witness,
            };
            let single_claim_proof = self
                .balance_prover
                .prove_single_claim(view_pair.view, self.config.is_faster_mining, &claim_witness)
                .await?;

            let collected_fees = match &fee {
                Some(fee) => {
                    let fee_beneficiary = fee_info.beneficiary.unwrap(); // already validated
                    select_unused_fees(
                        self.store_vault_server.as_ref(),
                        self.validity_prover.as_ref(),
                        view_pair,
                        fee_beneficiary,
                        fee.clone(),
                        FeeType::Claim,
                        self.config.tx_timeout,
                    )
                    .await?
                }
                None => vec![],
            };
            let fee_transfer_digests = collected_fees
                .iter()
                .map(|fee| fee.meta.digest)
                .collect::<Vec<_>>();

            // send claim request
            let fee_result = self
                .withdrawal_server
                .request_claim(
                    view_pair,
                    &single_claim_proof,
                    Some(fee_token_index),
                    &fee_transfer_digests,
                )
                .await?;

            match fee_result {
                FeeResult::Success => {}
                FeeResult::Insufficient => {
                    return Err(SyncError::FeeError(
                        "insufficient fee at the request".to_string(),
                    ))
                }
                FeeResult::TokenIndexMismatch => {
                    return Err(SyncError::FeeError(
                        "token index mismatch at the request".to_string(),
                    ))
                }
                _ => {
                    let reason = format!("fee error at the request: {fee_result:?}");
                    for used_fee in &collected_fees {
                        consume_payment(self.store_vault_server.as_ref(),  view_pair, used_fee, &reason)
                            .await?;
                    }
                    return Err(SyncError::FeeError(format!(
                        "invalid fee at the request: {fee_result:?}"
                    )));
                }
            }

            // consume fees
            for used_fee in &collected_fees {
                consume_payment(
                    self.store_vault_server.as_ref(),
                    view_pair,
                    used_fee,
                    "used for claim fee",
                )
                .await?;
            }

            // update user data
            let (mut user_data, prev_digest) = self.get_user_data_and_digest(view_pair).await?;
            user_data.claim_status.process(mining.meta.clone());

            // save user data
            self.save_user_data(view_pair, prev_digest, &user_data)
                .await?;

            log::info!("Claimed {}", mining.meta.digest.clone());
        }
        Ok(())
    }
}
