use intmax2_interfaces::{
    api::{
        store_vault_server::interface::StoreVaultClientInterface,
        validity_prover::interface::ValidityProverClientInterface,
    },
    data::{
        deposit_data::DepositData, meta_data::MetaData, transfer_data::TransferData,
        tx_data::TxData, user_data::UserData,
    },
};
use plonky2::{field::goldilocks_field::GoldilocksField, plonk::config::PoseidonGoldilocksConfig};

use intmax2_zkp::common::signature::key_set::KeySet;

use crate::{
    client::error::ClientError, external_api::contract::liquidity_contract::LiquidityContract,
};

use super::{deposit::fetch_deposit_info, transfer::fetch_transfer_info, tx::fetch_tx_info};

type F = GoldilocksField;
type C = PoseidonGoldilocksConfig;
const D: usize = 2;

// Next sync action
#[derive(Debug, Clone)]
pub enum Action {
    Deposit(MetaData, DepositData),            // Receive deposit
    Transfer(MetaData, TransferData<F, C, D>), // Receive transfer
    Tx(MetaData, TxData<F, C, D>),             // Send tx
    PendingTx(MetaData, TxData<F, C, D>),      // Pending tx
    None,
}

#[derive(Debug, Clone, Default)]
pub struct PendingInfo {
    pub pending_deposits: Vec<(MetaData, DepositData)>,
    pub pending_transfers: Vec<(MetaData, TransferData<F, C, D>)>,
}

// generate strategy of the balance proof update process
pub async fn determine_next_action<
    S: StoreVaultClientInterface,
    V: ValidityProverClientInterface,
>(
    store_vault_server: &S,
    validity_prover: &V,
    liquidity_contract: &LiquidityContract,
    key: KeySet,
    deposit_timeout: u64,
    tx_timeout: u64,
) -> Result<(Action, PendingInfo), ClientError> {
    // get user data from the data store server
    let user_data = store_vault_server
        .get_user_data(key.pubkey)
        .await?
        .map(|encrypted| UserData::decrypt(&encrypted, key))
        .transpose()
        .map_err(|e| ClientError::DecryptionError(e.to_string()))?
        .unwrap_or(UserData::new(key.pubkey));
    let prev_private_commitment = user_data.private_commitment();

    let tx_info = fetch_tx_info(
        store_vault_server,
        validity_prover,
        key,
        user_data.tx_lpt,
        tx_timeout,
    )
    .await?;

    // Check if there is a settled tx with the same prev_private_commitment
    for (meta, tx_data) in tx_info.settled.iter() {
        if tx_data.spent_witness.prev_private_state.commitment() == prev_private_commitment {
            return Ok((
                Action::Tx(meta.clone(), tx_data.clone()),
                PendingInfo::default(),
            ));
        }
    }

    // Check if there is a pending tx with the same prev_private_commitment
    for (meta, tx_data) in tx_info.pending.iter() {
        if tx_data.spent_witness.prev_private_state.commitment() == prev_private_commitment {
            return Ok((
                Action::PendingTx(meta.clone(), tx_data.clone()),
                PendingInfo::default(),
            ));
        }
    }

    let deposit_info = fetch_deposit_info(
        store_vault_server,
        validity_prover,
        liquidity_contract,
        key,
        user_data.deposit_lpt,
        deposit_timeout,
    )
    .await?;

    let transfer_info = fetch_transfer_info(
        store_vault_server,
        validity_prover,
        key,
        user_data.transfer_lpt,
        tx_timeout,
    )
    .await?;

    let pending_info = PendingInfo {
        pending_deposits: deposit_info.pending,
        pending_transfers: transfer_info.pending,
    };

    if transfer_info.settled.len() > 0 {
        // process from the latest transfer to reduce the number of updates
        let (meta, transfer_data) = transfer_info.settled.last().unwrap().clone();
        return Ok((Action::Transfer(meta, transfer_data), pending_info));
    } else if deposit_info.settled.len() > 0 {
        // process from the latest transfer to reduce the number of updates
        let (meta, deposit_data) = deposit_info.settled.last().unwrap().clone();
        return Ok((Action::Deposit(meta, deposit_data), pending_info));
    } else {
        return Ok((Action::None, pending_info));
    }
}
