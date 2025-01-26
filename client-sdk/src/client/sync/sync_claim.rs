use intmax2_interfaces::api::{
    balance_prover::interface::BalanceProverClientInterface,
    block_builder::interface::BlockBuilderClientInterface,
    store_vault_server::interface::StoreVaultClientInterface,
    validity_prover::interface::ValidityProverClientInterface,
    withdrawal_server::interface::WithdrawalServerClientInterface,
};
use intmax2_zkp::common::signature::key_set::KeySet;

use crate::client::{
    client::Client, strategy::strategy::determine_claim, sync::utils::get_balance_proof,
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
    /// Sync the client's withdrawals and relays to the withdrawal server
    pub async fn sync_claim(&self, key: KeySet) -> Result<(), SyncError> {
        if let Some((meta, deposit_data)) = determine_claim(
            &self.store_vault_server,
            &self.validity_prover,
            &self.liquidity_contract,
            key,
            self.config.deposit_timeout,
        )
        .await?
        {
            log::info!("sync_transfer: {:?}", meta);
            let (user_data, prev_digest) = self.get_user_data_and_digest(key).await?;
            let balance_proof = get_balance_proof(&user_data)?;

            todo!()
        } else {
            log::info!("No claim to sync");
        }
        Ok(())
    }

    // async fn sync_withdrawal(
    //     &self,
    //     key: KeySet,
    //     meta: MetaDataWithBlockNumber,
    //     withdrawal_data: &TransferData,
    // ) -> Result<(), SyncError> {
    //     log::info!("sync_withdrawal: {:?}", meta);
    //     // sender balance proof after applying the tx
    //     let balance_proof = match update_send_by_receiver(
    //         &self.validity_prover,
    //         &self.balance_prover,
    //         key,
    //         key.pubkey,
    //         meta.block_number,
    //         withdrawal_data,
    //     )
    //     .await
    //     {
    //         Ok(proof) => proof,
    //         Err(SyncError::InvalidTransferError(e)) => {
    //             log::error!(
    //                 "Ignore tx: {} because of invalid transfer: {}",
    //                 meta.meta.uuid,
    //                 e
    //             );
    //             return Ok(());
    //         }
    //         Err(e) => return Err(e),
    //     };

    //     let withdrawal_witness = WithdrawalWitness {
    //         transfer_witness: TransferWitness {
    //             transfer: withdrawal_data.transfer,
    //             transfer_index: withdrawal_data.transfer_index,
    //             transfer_merkle_proof: withdrawal_data.transfer_merkle_proof.clone(),
    //             tx: withdrawal_data.tx,
    //         },
    //         balance_proof,
    //     };
    //     let single_withdrawal_proof = self
    //         .balance_prover
    //         .prove_single_withdrawal(key, &withdrawal_witness)
    //         .await?;

    //     // send withdrawal request
    //     self.withdrawal_server
    //         .request_withdrawal(key.pubkey, &single_withdrawal_proof)
    //         .await?;

    //     // update user data
    //     let (mut user_data, prev_digest) = self.get_user_data_and_digest(key).await?;
    //     user_data.withdrawal_status.process(meta.meta);

    //     self.store_vault_server
    //         .save_user_data(key, prev_digest, &user_data.encrypt(key.pubkey))
    //         .await?;

    //     Ok(())
    // }
}
