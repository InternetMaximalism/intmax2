use intmax2_interfaces::{
    api::{
        balance_prover::interface::BalanceProverClientInterface,
        block_builder::interface::BlockBuilderClientInterface,
        store_vault_server::interface::StoreVaultClientInterface,
        validity_prover::interface::ValidityProverClientInterface,
        withdrawal_server::interface::WithdrawalServerClientInterface,
    },
    data::{
        deposit_data::DepositData, encryption::Encryption as _, meta_data::MetaDataWithBlockNumber,
        proof_compression::CompressedBalanceProof, transfer_data::TransferData, tx_data::TxData,
        user_data::UserData,
    },
    utils::digest::get_digest,
};
use intmax2_zkp::{
    circuits::balance::balance_pis::BalancePublicInputs, common::signature::key_set::KeySet,
    ethereum_types::bytes32::Bytes32,
};

use crate::client::{
    client::Client,
    strategy::strategy::{determine_sequence, Action, PendingInfo, ReceiveAction},
    sync::{
        balance_logic::{
            receive_deposit, receive_transfer, update_no_send, update_send_by_receiver,
            update_send_by_sender,
        },
        utils::{generate_salt, get_balance_proof},
    },
};

use super::error::SyncError;

impl<BB, S, V, B, W> Client<BB, S, V, B, W>
where
    BB: BlockBuilderClientInterface,
    S: StoreVaultClientInterface,
    V: ValidityProverClientInterface,
    B: BalanceProverClientInterface,
    W: WithdrawalServerClientInterface,
{
    /// Get the latest user data from the data store server
    pub async fn get_user_data_and_digest(
        &self,
        key: KeySet,
    ) -> Result<(UserData, Option<Bytes32>), SyncError> {
        let encrypted_data = self.store_vault_server.get_user_data(key).await?;
        let digest = encrypted_data
            .as_ref()
            .map(|encrypted| get_digest(encrypted));
        let user_data = encrypted_data
            .map(|encrypted| UserData::decrypt(&encrypted, key))
            .transpose()
            .map_err(|e| SyncError::DecryptionError(format!("failed to decrypt user data: {}", e)))?
            .unwrap_or(UserData::new(key.pubkey));
        Ok((user_data, digest))
    }

    /// Sync the client's balance proof with the latest block
    pub async fn sync(&self, key: KeySet) -> Result<(), SyncError> {
        let (sequence, pending_info) = determine_sequence(
            &self.store_vault_server,
            &self.validity_prover,
            &self.liquidity_contract,
            key,
            self.config.deposit_timeout,
            self.config.tx_timeout,
        )
        .await?;
        // replaces pending receives with the new pending info
        self.update_pending_receives(key, pending_info).await?;

        for action in sequence {
            match action {
                Action::Receive(receives) => {
                    if !receives.is_empty() {
                        // Update the balance proof with the largest block in the receives
                        let largest_block_number = receives
                            .iter()
                            .map(|r| r.meta().block_number)
                            .max()
                            .unwrap(); // safe to unwrap because receives is not empty
                        self.update_no_send(key, largest_block_number).await?;

                        for receive in receives {
                            match receive {
                                ReceiveAction::Deposit(meta, data) => {
                                    self.sync_deposit(key, meta, &data).await?;
                                }
                                ReceiveAction::Transfer(meta, data) => {
                                    self.sync_transfer(key, meta, &data).await?;
                                }
                            }
                        }
                    }
                }
                Action::Tx(meta, tx_data) => {
                    self.sync_tx(key, meta, &tx_data).await?;
                }
            }
        }
        Ok(())
    }

    // sync deposit without updating the timestamp
    async fn sync_deposit(
        &self,
        key: KeySet,
        meta: MetaDataWithBlockNumber,
        deposit_data: &DepositData,
    ) -> Result<(), SyncError> {
        log::info!("sync_deposit: {:?}", meta);
        let (mut user_data, prev_digest) = self.get_user_data_and_digest(key).await?;
        // user's balance proof before applying the tx
        let prev_balance_proof = get_balance_proof(&user_data)?;
        let new_salt = generate_salt();
        let new_balance_proof = receive_deposit(
            &self.validity_prover,
            &self.balance_prover,
            key,
            &mut user_data.full_private_state,
            new_salt,
            &prev_balance_proof,
            deposit_data,
        )
        .await?;
        // validation
        let new_balance_pis = BalancePublicInputs::from_pis(&new_balance_proof.public_inputs);
        if new_balance_pis.private_commitment != user_data.private_commitment() {
            return Err(SyncError::InternalError(
                "private commitment mismatch".to_string(),
            ));
        }
        let new_balance_proof = CompressedBalanceProof::new(&new_balance_proof)?;
        // update user data
        user_data.balance_proof = Some(new_balance_proof);
        user_data.deposit_status.process(meta.meta);
        // save user data
        self.store_vault_server
            .save_user_data(key, prev_digest, &user_data.encrypt(key.pubkey))
            .await?;
        Ok(())
    }

    // sync deposit without updating the timestamp
    async fn sync_transfer(
        &self,
        key: KeySet,
        meta: MetaDataWithBlockNumber,
        transfer_data: &TransferData,
    ) -> Result<(), SyncError> {
        log::info!("sync_transfer: {:?}", meta);
        let (mut user_data, prev_digest) = self.get_user_data_and_digest(key).await?;
        // user's balance proof before applying the tx
        let prev_balance_proof = get_balance_proof(&user_data)?;

        // sender balance proof after applying the tx
        let new_sender_balance_proof = match update_send_by_receiver(
            &self.validity_prover,
            &self.balance_prover,
            key,
            transfer_data.sender,
            meta.block_number,
            transfer_data,
        )
        .await
        {
            Ok(proof) => proof,
            Err(SyncError::InvalidTransferError(e)) => {
                log::error!(
                    "Ignore tx: {} because of invalid transfer: {}",
                    meta.meta.uuid,
                    e
                );
                return Ok(());
            }
            Err(e) => return Err(e),
        };

        let new_salt = generate_salt();
        let new_balance_proof = receive_transfer(
            &self.validity_prover,
            &self.balance_prover,
            key,
            &mut user_data.full_private_state,
            new_salt,
            &new_sender_balance_proof,
            &prev_balance_proof,
            transfer_data,
        )
        .await?;
        let new_balance_pis = BalancePublicInputs::from_pis(&new_balance_proof.public_inputs);
        if new_balance_pis.private_commitment != user_data.private_commitment() {
            return Err(SyncError::InternalError(
                "private commitment mismatch".to_string(),
            ));
        }

        // update user data
        let balance_proof = CompressedBalanceProof::new(&new_balance_proof)?;
        user_data.balance_proof = Some(balance_proof);
        user_data.transfer_status.process(meta.meta);

        // save proof and user data
        self.store_vault_server
            .save_user_data(key, prev_digest, &user_data.encrypt(key.pubkey))
            .await?;

        Ok(())
    }

    async fn sync_tx(
        &self,
        key: KeySet,
        meta: MetaDataWithBlockNumber,
        tx_data: &TxData,
    ) -> Result<(), SyncError> {
        log::info!("sync_tx: {:?}", meta);
        let (mut user_data, digest) = self.get_user_data_and_digest(key).await?;
        let prev_balance_proof = get_balance_proof(&user_data)?;
        let balance_proof = update_send_by_sender(
            &self.validity_prover,
            &self.balance_prover,
            key,
            &mut user_data.full_private_state,
            &prev_balance_proof,
            meta.block_number,
            tx_data,
        )
        .await?;
        let balance_pis = BalancePublicInputs::from_pis(&balance_proof.public_inputs);
        // validation
        if balance_pis.public_state.block_number != meta.block_number {
            return Err(SyncError::BalanceProofBlockNumberMismatch {
                balance_proof_block_number: balance_pis.public_state.block_number,
                block_number: meta.block_number,
            });
        }
        if balance_pis.private_commitment != user_data.private_commitment() {
            return Err(SyncError::InternalError(
                "private commitment mismatch".to_string(),
            ));
        }

        // update user data
        let balance_proof = CompressedBalanceProof::new(&balance_proof)?;
        user_data.balance_proof = Some(balance_proof);
        user_data.tx_status.process(meta.meta);

        // save user data
        self.store_vault_server
            .save_user_data(key, digest, &user_data.encrypt(key.pubkey))
            .await?;
        Ok(())
    }

    async fn update_no_send(&self, key: KeySet, to_block_number: u32) -> Result<(), SyncError> {
        log::info!("update_no_send: {:?}", to_block_number);
        let (mut user_data, digest) = self.get_user_data_and_digest(key).await?;
        log::info!(
            "update_no_send: user_data.block_number {},  to_block_number {}",
            user_data.block_number()?,
            to_block_number
        );
        let prev_balance_proof = get_balance_proof(&user_data)?;
        let new_balance_proof = update_no_send(
            &self.validity_prover,
            &self.balance_prover,
            key,
            &prev_balance_proof,
            to_block_number,
        )
        .await?;
        let new_balance_pis = BalancePublicInputs::from_pis(&new_balance_proof.public_inputs);
        let new_block_number = new_balance_pis.public_state.block_number;
        if new_block_number != to_block_number {
            return Err(SyncError::BalanceProofBlockNumberMismatch {
                balance_proof_block_number: new_block_number,
                block_number: to_block_number,
            });
        }
        if new_balance_pis.private_commitment != user_data.private_commitment() {
            return Err(SyncError::InternalError(
                "private commitment mismatch".to_string(),
            ));
        }
        // update user data
        let balance_proof = CompressedBalanceProof::new(&new_balance_proof)?;
        user_data.balance_proof = Some(balance_proof);
        self.store_vault_server
            .save_user_data(key, digest, &user_data.encrypt(key.pubkey))
            .await?;

        Ok(())
    }

    async fn update_pending_receives(
        &self,
        key: KeySet,
        pending_info: PendingInfo,
    ) -> Result<(), SyncError> {
        let (mut user_data, prev_digest) = self.get_user_data_and_digest(key).await?;
        user_data.deposit_status.pending_uuids = pending_info.pending_deposit_uuids;
        user_data.transfer_status.pending_uuids = pending_info.pending_transfer_uuids;
        self.store_vault_server
            .save_user_data(key, prev_digest, &user_data.encrypt(key.pubkey))
            .await?;
        Ok(())
    }
}
