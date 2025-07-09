use intmax2_client_sdk::{
    client::{
        fee_payment::FeeType,
        receive_validation::{validate_receive, ReceiveValidationError},
    },
    external_api::validity_prover::ValidityProverClient,
};
use intmax2_interfaces::{
    api::{
        store_vault_server::interface::StoreVaultClientInterface,
        withdrawal_server::interface::FeeResult,
    },
    data::{
        data_type::DataType,
        encryption::{errors::BlsEncryptionError, BlsEncryption},
        transfer_data::TransferData,
    },
    utils::fee::Fee,
};
use intmax2_zkp::{
    common::transfer::Transfer,
    ethereum_types::{bytes32::Bytes32, u256::U256, u32limb_trait::U32LimbTrait},
};

use super::{config::Config, db_operations::DbOperations, error::WithdrawalServerError};

pub struct FeeHandler<'a> {
    config: &'a Config,
    db_operations: DbOperations<'a>,
    store_vault_server: &'a dyn StoreVaultClientInterface,
    validity_prover: &'a ValidityProverClient,
}

impl<'a> FeeHandler<'a> {
    pub fn new(
        config: &'a Config,
        db_operations: DbOperations<'a>,
        store_vault_server: &'a dyn StoreVaultClientInterface,
        validity_prover: &'a ValidityProverClient,
    ) -> Self {
        Self {
            config,
            db_operations,
            store_vault_server,
            validity_prover,
        }
    }

    pub async fn validate_fee(
        &self,
        fee_type: FeeType,
        fee: &Fee,
        fee_transfer_digests: &[Bytes32],
    ) -> Result<(Vec<Transfer>, FeeResult), WithdrawalServerError> {
        // check duplicated nullifiers

        let view_pair = match fee_type {
            FeeType::Withdrawal => self.config.withdrawal_beneficiary_key,
            FeeType::Claim => self.config.claim_beneficiary_key,
        };
        // fetch transfer data
        let encrypted_transfer_data = self
            .store_vault_server
            .get_data_batch(
                view_pair.view,
                &DataType::Transfer.to_topic(),
                fee_transfer_digests,
            )
            .await?;
        if encrypted_transfer_data.len() != fee_transfer_digests.len() {
            return Err(WithdrawalServerError::InvalidFee(format!(
                "Invalid fee transfer digest response: expected {}, got {}",
                fee_transfer_digests.len(),
                encrypted_transfer_data.len()
            )));
        }

        let transfer_data_with_meta = encrypted_transfer_data
            .iter()
            .map(|data| {
                let transfer_data = TransferData::decrypt(view_pair.view, None, &data.data)?;
                Ok((data.meta.clone(), transfer_data))
            })
            .collect::<Result<Vec<_>, BlsEncryptionError>>();
        let transfer_data_with_meta = match transfer_data_with_meta {
            Ok(data) => data,
            Err(e) => {
                log::warn!("Failed to decrypt transfer data: {e}");
                return Ok((Vec::new(), FeeResult::DecryptionError));
            }
        };

        let mut collected_fee = U256::zero();
        let mut transfers = Vec::new();
        for (meta, transfer_data) in transfer_data_with_meta {
            let transfer = match validate_receive(
                self.store_vault_server,
                self.validity_prover,
                view_pair.spend,
                meta.timestamp,
                &transfer_data,
            )
            .await
            {
                Ok(transfer) => transfer,
                Err(e) => {
                    if matches!(e, ReceiveValidationError::ValidationError(_)) {
                        return Ok((Vec::new(), FeeResult::ValidationError));
                    } else {
                        return Err(e.into());
                    }
                }
            };
            if fee.token_index != transfer.token_index {
                return Ok((Vec::new(), FeeResult::TokenIndexMismatch));
            }
            collected_fee += transfer.amount;
            transfers.push(transfer);
        }
        if collected_fee < fee.amount {
            return Ok((Vec::new(), FeeResult::Insufficient));
        }
        if !self
            .db_operations
            .check_no_duplicated_nullifiers(&transfers)
            .await?
        {
            return Ok((Vec::new(), FeeResult::AlreadyUsed));
        }
        Ok((transfers, FeeResult::Success))
    }
}
