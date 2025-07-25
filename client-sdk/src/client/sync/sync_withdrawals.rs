use crate::client::{
    client::Client,
    fee_payment::{consume_payment, select_unused_fees, FeeType},
    strategy::strategy::determine_withdrawals,
    sync::{balance_logic::update_send_by_receiver, utils::quote_withdrawal_claim_fee},
};
use intmax2_interfaces::{
    api::withdrawal_server::interface::{FeeResult, WithdrawalFeeInfo},
    data::{meta_data::MetaDataWithBlockNumber, transfer_data::TransferData},
    utils::{address::IntmaxAddress, fee::Fee, key::ViewPair},
};
use intmax2_zkp::{
    common::witness::{transfer_witness::TransferWitness, withdrawal_witness::WithdrawalWitness},
    ethereum_types::bytes32::Bytes32,
};

use super::error::SyncError;

impl Client {
    /// Sync the client's withdrawals and relays to the withdrawal server
    pub async fn sync_withdrawals(
        &self,
        view_pair: ViewPair,
        withdrawal_fee: &WithdrawalFeeInfo,
        fee_token_index: u32,
    ) -> Result<(), SyncError> {
        let fee_beneficiary = withdrawal_fee.beneficiary;
        let (withdrawals, pending) = determine_withdrawals(
            self.store_vault_server.as_ref(),
            self.validity_prover.as_ref(),
            self.withdrawal_server.as_ref(),
            &self.rollup_contract,
            view_pair,
            self.config.tx_timeout,
        )
        .await?;
        self.update_pending_withdrawals(view_pair, pending).await?;
        for (meta, data) in withdrawals {
            self.sync_withdrawal(
                view_pair,
                meta,
                &data,
                fee_beneficiary,
                fee_token_index,
                withdrawal_fee.direct_withdrawal_fee.clone(),
                withdrawal_fee.claimable_withdrawal_fee.clone(),
            )
            .await?;
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn sync_withdrawal(
        &self,
        view_pair: ViewPair,
        meta: MetaDataWithBlockNumber,
        withdrawal_data: &TransferData,
        fee_beneficiary: IntmaxAddress,
        fee_token_index: u32,
        direct_withdrawal_fee: Option<Vec<Fee>>,
        claimable_withdrawal_fee: Option<Vec<Fee>>,
    ) -> Result<(), SyncError> {
        log::info!("sync_withdrawal: {meta:?}");
        // sender balance proof after applying the tx
        let balance_proof = match update_send_by_receiver(
            self.validity_prover.as_ref(),
            self.balance_prover.as_ref(),
            view_pair,
            view_pair.spend,
            meta.block_number,
            withdrawal_data,
        )
        .await
        {
            Ok(proof) => proof,
            Err(SyncError::InvalidTransferError(e)) => {
                log::error!(
                    "Ignore tx: {} because of invalid transfer: {}",
                    meta.meta.digest,
                    e
                );
                return Ok(());
            }
            Err(e) => return Err(e),
        };

        let withdrawal_witness = WithdrawalWitness {
            transfer_witness: TransferWitness {
                transfer: withdrawal_data.transfer,
                transfer_index: withdrawal_data.transfer_index,
                transfer_merkle_proof: withdrawal_data.transfer_merkle_proof.clone(),
                tx: withdrawal_data.tx,
            },
            balance_proof,
        };
        let single_withdrawal_proof = self
            .balance_prover
            .prove_single_withdrawal(view_pair.view, &withdrawal_witness)
            .await?;

        let direct_withdrawal_indices = self
            .withdrawal_contract
            .get_direct_withdrawal_token_indices()
            .await?;
        let fee = if direct_withdrawal_indices.contains(&withdrawal_data.transfer.token_index) {
            quote_withdrawal_claim_fee(Some(fee_token_index), direct_withdrawal_fee.clone())?
        } else {
            quote_withdrawal_claim_fee(Some(fee_token_index), claimable_withdrawal_fee.clone())?
        };

        let collected_fees = match &fee {
            Some(fee) => {
                let fee_beneficiary = fee_beneficiary.public_spend.0;
                select_unused_fees(
                    self.store_vault_server.as_ref(),
                    self.validity_prover.as_ref(),
                    view_pair,
                    fee_beneficiary,
                    fee.clone(),
                    FeeType::Withdrawal,
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

        // send withdrawal request
        let fee_result = self
            .withdrawal_server
            .request_withdrawal(
                view_pair.view,
                &single_withdrawal_proof,
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
                    consume_payment(
                        self.store_vault_server.as_ref(),
                        view_pair,
                        used_fee,
                        &reason,
                    )
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
                "used for withdrawal fee",
            )
            .await?;
        }

        // update user data
        let (mut user_data, prev_digest) = self.get_user_data_and_digest(view_pair).await?;
        user_data.withdrawal_status.process(meta.meta);

        // save user data
        self.save_user_data(view_pair, prev_digest, &user_data)
            .await?;

        Ok(())
    }

    async fn update_pending_withdrawals(
        &self,
        view_pair: ViewPair,
        pending_withdrawal_digests: Vec<Bytes32>,
    ) -> Result<(), SyncError> {
        if pending_withdrawal_digests.is_empty() {
            // no pending withdrawals
            return Ok(());
        }
        let (mut user_data, prev_digest) = self.get_user_data_and_digest(view_pair).await?;
        user_data.withdrawal_status.pending_digests = pending_withdrawal_digests;
        // save user data
        self.save_user_data(view_pair, prev_digest, &user_data)
            .await?;
        Ok(())
    }
}
