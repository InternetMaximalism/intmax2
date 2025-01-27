use intmax2_interfaces::{
    api::{
        balance_prover::interface::BalanceProverClientInterface,
        block_builder::interface::BlockBuilderClientInterface,
        store_vault_server::interface::StoreVaultClientInterface,
        validity_prover::interface::ValidityProverClientInterface,
        withdrawal_server::interface::WithdrawalServerClientInterface,
    },
    data::encryption::Encryption as _,
};
use intmax2_zkp::{
    common::{
        signature::key_set::KeySet,
        witness::{
            claim_witness::ClaimWitness,
            deposit_time_witness::{DepositTimePublicWitness, DepositTimeWitness},
        },
    },
    ethereum_types::address::Address,
};

use crate::client::{client::Client, strategy::strategy::determine_claim};

use super::error::SyncError;

impl<BB, S, V, B, W> Client<BB, S, V, B, W>
where
    BB: BlockBuilderClientInterface,
    S: StoreVaultClientInterface,
    V: ValidityProverClientInterface,
    B: BalanceProverClientInterface,
    W: WithdrawalServerClientInterface,
{
    /// Sync the client's withdrawals and relays to the withdrawal server
    pub async fn sync_claim(&self, key: KeySet, recipient: Address) -> Result<(), SyncError> {
        if let Some(mining) = determine_claim(
            &self.store_vault_server,
            &self.validity_prover,
            &self.liquidity_contract,
            key,
            self.config.deposit_timeout,
        )
        .await?
        {
            log::info!("sync_claim: {:?}", mining.meta);

            // update to current block number
            let current_block_number = self.validity_prover.get_block_number().await?;
            self.update_no_send(key, current_block_number).await?;

            // check the block number
            let user_data = self.get_user_data(key).await?;
            if user_data.block_number()? != current_block_number {
                return Err(SyncError::BalanceProofBlockNumberMismatch {
                    balance_proof_block_number: user_data.block_number()?,
                    block_number: current_block_number,
                });
            }

            // collect witnesses
            let deposit_block_number = mining.block.block_number;
            let update_witness = self
                .validity_prover
                .get_update_witness(
                    key.pubkey,
                    current_block_number,
                    deposit_block_number,
                    false,
                )
                .await?;
            let last_block_number = update_witness.account_membership_proof.get_value() as u32;
            if deposit_block_number <= last_block_number {
                return Err(SyncError::InternalError(format!(
                    "deposit block number {} is less than last block number {}",
                    deposit_block_number, last_block_number
                )));
            }
            let deposit_hash = mining.deposit_data.deposit_hash().unwrap(); // safe to unwrap because it's already settled
            let deposit_info = self
                .validity_prover
                .get_deposit_info(deposit_hash)
                .await?
                .ok_or(SyncError::DepositInfoNotFound(deposit_hash))?;
            let prev_block = self
                .validity_prover
                .get_validity_witness(deposit_block_number - 1)
                .await?
                .block_witness
                .block;
            let prev_deposit_merkle_proof = self
                .validity_prover
                .get_deposit_merkle_proof(deposit_block_number - 1, deposit_info.deposit_index)
                .await?;
            let deposit_merkle_proof = self
                .validity_prover
                .get_deposit_merkle_proof(deposit_block_number, deposit_info.deposit_index)
                .await?;
            let public_witness = DepositTimePublicWitness {
                prev_block,
                block: mining.block,
                prev_deposit_merkle_proof,
                deposit_merkle_proof,
            };
            let deposit_time_witness = DepositTimeWitness {
                public_witness,
                deposit_index: deposit_info.deposit_index,
                deposit: mining.deposit_data.deposit().unwrap(),
                deposit_salt: mining.deposit_data.deposit_salt,
                pubkey: key.pubkey,
            };
            let claim_witness = ClaimWitness {
                recipient,
                deposit_time_witness,
                update_witness,
            };
            let single_claim_proof = self
                .balance_prover
                .prove_single_claim(key, &claim_witness)
                .await?;

            // send claim request
            self.withdrawal_server
                .request_claim(key, &single_claim_proof)
                .await?;

            // update user data
            let (mut user_data, prev_digest) = self.get_user_data_and_digest(key).await?;
            user_data.claim_status.process(mining.meta.meta.clone());

            self.store_vault_server
                .save_user_data(key, prev_digest, &user_data.encrypt(key.pubkey))
                .await?;
            log::info!("Claimed {}", mining.meta.meta.uuid.clone());
        } else {
            log::info!("No claim to sync");
        }
        Ok(())
    }
}
