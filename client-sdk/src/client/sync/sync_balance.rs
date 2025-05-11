use intmax2_interfaces::{
    data::{
        data_type::DataType, deposit_data::DepositData, encryption::BlsEncryption as _,
        meta_data::MetaDataWithBlockNumber, proof_compression::CompressedBalanceProof,
        transfer_data::TransferData, tx_data::TxData, user_data::UserData,
    },
    utils::digest::get_digest,
};
use intmax2_zkp::{
    circuits::balance::balance_pis::BalancePublicInputs,
    common::signature_content::key_set::KeySet, ethereum_types::bytes32::Bytes32,
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

impl Client {
    pub async fn get_user_data(&self, key: KeySet) -> Result<UserData, SyncError> {
        let (user_data, _) = self.get_user_data_and_digest(key).await?;
        Ok(user_data)
    }

    /// Get the latest user data from the data store server
    pub(super) async fn get_user_data_and_digest(
        &self,
        key: KeySet,
    ) -> Result<(UserData, Option<Bytes32>), SyncError> {
        let encrypted_data = self
            .store_vault_server
            .get_snapshot(key, &DataType::UserData.to_topic())
            .await?;
        let digest = encrypted_data
            .as_ref()
            .map(|encrypted| get_digest(encrypted));
        let user_data = encrypted_data
            .map(|encrypted| UserData::decrypt(key, Some(key.pubkey), &encrypted))
            .transpose()
            .map_err(|e| SyncError::DecryptionError(format!("failed to decrypt user data: {}", e)))?
            .unwrap_or(UserData::new(key.pubkey));
        Ok((user_data, digest))
    }

    pub(super) async fn save_user_data(
        &self,
        key: KeySet,
        prev_digest: Option<Bytes32>,
        user_data: &UserData,
    ) -> Result<(), SyncError> {
        let encrypted_data = user_data.encrypt(key.pubkey, Some(key))?;
        self.store_vault_server
            .save_snapshot(
                key,
                &DataType::UserData.to_topic(),
                prev_digest,
                &encrypted_data,
            )
            .await?;
        Ok(())
    }

    /// Sync the client's balance proof with the latest block
    pub async fn sync(&self, key: KeySet) -> Result<(), SyncError> {
        let (sequence, _, pending_info) = determine_sequence(
            self.store_vault_server.as_ref(),
            self.validity_prover.as_ref(),
            &self.rollup_contract,
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
        let nullifier: Bytes32 = deposit_data.deposit().unwrap().poseidon_hash().into();
        if user_data
            .full_private_state
            .nullifier_tree
            .nullifiers()
            .contains(&nullifier)
        {
            log::error!(
                "Ignore deposit: {} because of nullifier: {} already exists",
                meta.meta.digest,
                nullifier
            );
            return Ok(());
        }
        // user's balance proof before applying the tx
        let prev_balance_proof = get_balance_proof(&user_data)?;
        let new_salt = generate_salt();
        let new_balance_proof = receive_deposit(
            self.validity_prover.as_ref(),
            self.balance_prover.as_ref(),
            key,
            &mut user_data.full_private_state,
            new_salt,
            &prev_balance_proof,
            deposit_data,
        )
        .await?;
        // validation
        let new_balance_pis = BalancePublicInputs::from_pis(&new_balance_proof.public_inputs)?;
        if new_balance_pis.private_commitment != user_data.private_commitment() {
            return Err(SyncError::InternalError(
                "private commitment mismatch".to_string(),
            ));
        }
        // update user data
        let new_balance_proof = CompressedBalanceProof::new(&new_balance_proof)?;
        user_data.balance_proof = Some(new_balance_proof);
        user_data.deposit_status.process(meta.meta);
        self.save_user_data(key, prev_digest, &user_data).await?;

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
        // nullifier check
        let nullifier = transfer_data.transfer.nullifier();
        if user_data
            .full_private_state
            .nullifier_tree
            .nullifiers()
            .contains(&nullifier)
        {
            log::error!(
                "Ignore tx: {} because of nullifier: {} already exists",
                meta.meta.digest,
                nullifier
            );
            return Ok(());
        }

        // user's balance proof before applying the tx
        let prev_balance_proof = get_balance_proof(&user_data)?;

        // sender balance proof after applying the tx
        let new_sender_balance_proof = match update_send_by_receiver(
            self.validity_prover.as_ref(),
            self.balance_prover.as_ref(),
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
                    meta.meta.digest,
                    e
                );
                return Ok(());
            }
            Err(e) => return Err(e),
        };

        let new_salt = generate_salt();
        let new_balance_proof = receive_transfer(
            self.validity_prover.as_ref(),
            self.balance_prover.as_ref(),
            key,
            &mut user_data.full_private_state,
            new_salt,
            &new_sender_balance_proof,
            &prev_balance_proof,
            transfer_data,
        )
        .await?;
        let new_balance_pis = BalancePublicInputs::from_pis(&new_balance_proof.public_inputs)?;
        if new_balance_pis.private_commitment != user_data.private_commitment() {
            return Err(SyncError::InternalError(
                "private commitment mismatch".to_string(),
            ));
        }

        // update user data
        let balance_proof = CompressedBalanceProof::new(&new_balance_proof)?;
        user_data.balance_proof = Some(balance_proof);
        user_data.transfer_status.process(meta.meta);
        self.save_user_data(key, prev_digest, &user_data).await?;

        Ok(())
    }

    async fn sync_tx(
        &self,
        key: KeySet,
        meta: MetaDataWithBlockNumber,
        tx_data: &TxData,
    ) -> Result<(), SyncError> {
        log::info!("sync_tx: {:?}", meta);
        let (mut user_data, prev_digest) = self.get_user_data_and_digest(key).await?;
        let prev_balance_proof = get_balance_proof(&user_data)?;
        let balance_proof = update_send_by_sender(
            self.validity_prover.as_ref(),
            self.balance_prover.as_ref(),
            key,
            &mut user_data.full_private_state,
            &prev_balance_proof,
            meta.block_number,
            tx_data,
        )
        .await?;
        let balance_pis = BalancePublicInputs::from_pis(&balance_proof.public_inputs)?;
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
        self.save_user_data(key, prev_digest, &user_data).await?;

        Ok(())
    }

    pub(super) async fn update_no_send(
        &self,
        key: KeySet,
        to_block_number: u32,
    ) -> Result<(), SyncError> {
        log::info!("update_no_send: {:?}", to_block_number);
        let (mut user_data, prev_digest) = self.get_user_data_and_digest(key).await?;
        let current_user_block_number = user_data.block_number()?;
        if current_user_block_number >= to_block_number {
            log::info!(
                "No need to update: current {} >= to {}",
                current_user_block_number,
                to_block_number
            );
            return Ok(());
        }
        log::info!(
            "update_no_send: user_data.block_number {},  to_block_number {}",
            user_data.block_number()?,
            to_block_number
        );
        let prev_balance_proof = get_balance_proof(&user_data)?;
        let new_balance_proof = update_no_send(
            self.validity_prover.as_ref(),
            self.balance_prover.as_ref(),
            key,
            &prev_balance_proof,
            to_block_number,
        )
        .await?;
        let new_balance_pis = BalancePublicInputs::from_pis(&new_balance_proof.public_inputs)?;
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
        self.save_user_data(key, prev_digest, &user_data).await?;

        Ok(())
    }

    async fn update_pending_receives(
        &self,
        key: KeySet,
        pending_info: PendingInfo,
    ) -> Result<(), SyncError> {
        if pending_info.pending_deposit_digests.is_empty()
            && pending_info.pending_transfer_digests.is_empty()
        {
            // // early return if there is no pending info
            return Ok(());
        }
        let (mut user_data, prev_digest) = self.get_user_data_and_digest(key).await?;
        user_data.deposit_status.pending_digests = pending_info.pending_deposit_digests;
        user_data.transfer_status.pending_digests = pending_info.pending_transfer_digests;
        self.save_user_data(key, prev_digest, &user_data).await?;
        Ok(())
    }

    /// Reset user data and resync. `is_deep` is true if the user wants to reset completely.
    /// Otherwise, only the last processed meta data will be reset.
    pub async fn resync(&self, key: KeySet, is_deep: bool) -> Result<(), SyncError> {
        let (mut user_data, prev_digest) = self.get_user_data_and_digest(key).await?;

        if is_deep {
            user_data = UserData::new(key.pubkey);
        } else {
            user_data.deposit_status.last_processed_meta_data = None;
            user_data.transfer_status.last_processed_meta_data = None;
            user_data.withdrawal_status.last_processed_meta_data = None;
        }

        self.save_user_data(key, prev_digest, &user_data).await?;

        self.sync(key).await
    }
}
