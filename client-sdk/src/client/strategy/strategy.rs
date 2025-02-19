use intmax2_interfaces::{
    api::{
        store_vault_server::interface::StoreVaultClientInterface,
        validity_prover::interface::ValidityProverClientInterface,
    },
    data::{
        deposit_data::DepositData, meta_data::MetaDataWithBlockNumber, transfer_data::TransferData,
        tx_data::TxData, user_data::Balances,
    },
};
use itertools::Itertools;

use intmax2_zkp::common::signature::key_set::KeySet;

use crate::{
    client::strategy::{
        common::fetch_user_data,
        deposit::fetch_all_unprocessed_deposit_info,
        mining::{fetch_mining_info, MiningStatus},
        transfer::fetch_all_unprocessed_transfer_info,
        tx::fetch_all_unprocessed_tx_info,
        withdrawal::fetch_all_unprocessed_withdrawal_info,
    },
    external_api::contract::liquidity_contract::LiquidityContract,
};

use super::{error::StrategyError, mining::Mining};

// Next sync action
#[derive(Debug, Clone)]
pub enum Action {
    Receive(Vec<ReceiveAction>),
    Tx(MetaDataWithBlockNumber, Box<TxData>), // Send tx
}

#[derive(Debug, Clone)]
pub enum ReceiveAction {
    Deposit(MetaDataWithBlockNumber, DepositData),
    Transfer(MetaDataWithBlockNumber, Box<TransferData>), // Boxed to avoid large stack size
}

impl ReceiveAction {
    pub fn meta(&self) -> &MetaDataWithBlockNumber {
        match self {
            ReceiveAction::Deposit(meta, _) => meta,
            ReceiveAction::Transfer(meta, _) => meta,
        }
    }

    pub fn apply_to_balances(&self, balances: &mut Balances) {
        match self {
            ReceiveAction::Deposit(_, data) => {
                balances.add_deposit(data);
            }
            ReceiveAction::Transfer(_, data) => {
                balances.add_transfer(data);
            }
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct PendingInfo {
    pub pending_deposit_uuids: Vec<String>,
    pub pending_transfer_uuids: Vec<String>,
}

/// Determine the sequence of receives/send tx to be incorporated into the balance proof
pub async fn determine_sequence<S: StoreVaultClientInterface, V: ValidityProverClientInterface>(
    store_vault_server: &S,
    validity_prover: &V,
    liquidity_contract: &LiquidityContract,
    key: KeySet,
    deposit_timeout: u64,
    tx_timeout: u64,
) -> Result<(Vec<Action>, PendingInfo), StrategyError> {
    log::info!("determine_sequence");
    let user_data = fetch_user_data(store_vault_server, key).await?;
    let mut balances = user_data.balances();
    if balances.is_insufficient() {
        return Err(StrategyError::BalanceInsufficientBeforeSync);
    }

    let tx_info = fetch_all_unprocessed_tx_info(
        store_vault_server,
        validity_prover,
        key,
        &user_data.tx_status,
        tx_timeout,
    )
    .await?;

    //  First, if there is a pending tx, return a pending error
    if let Some((meta, _tx_data)) = tx_info.pending.first() {
        return Err(StrategyError::PendingTxError(format!(
            "pending tx: {:?}",
            meta.uuid
        )));
    }

    // Then, collect deposit and transfer data
    let deposit_info = fetch_all_unprocessed_deposit_info(
        store_vault_server,
        validity_prover,
        liquidity_contract,
        key,
        &user_data.deposit_status,
        deposit_timeout,
    )
    .await?;
    let transfer_info = fetch_all_unprocessed_transfer_info(
        store_vault_server,
        validity_prover,
        key,
        &user_data.transfer_status,
        tx_timeout,
    )
    .await?;

    let mut deposits = deposit_info.settled;
    let mut transfers = transfer_info.settled;

    // Next, for each settled tx, take deposits and transfers that are strictly smaller than the block number of the tx
    let mut sequence = Vec::new();
    for (tx_meta, tx_data) in tx_info.settled.iter() {
        let receives = collect_receives(
            &Some((tx_meta.clone(), tx_data.clone())),
            &mut deposits,
            &mut transfers,
        )
        .await?;

        // Apply receives to balances
        for receive in &receives {
            receive.apply_to_balances(&mut balances);
        }
        let is_insufficient = balances.sub_tx(tx_data);
        if is_insufficient {
            if deposit_info.pending.is_empty() && transfer_info.pending.is_empty() {
                // Unresolved balance shortage
                return Err(StrategyError::BalanceInsufficientDuringSync);
            } else {
                // To incorporate the tx, you need to incorporate the pending deposit/transfer to solve the balance shortage.
                // TODO: Processing when the balance shortage is not resolved even if the pending deposit/transfer is incorporated
                return Err(StrategyError::PendingReceivesError(format!(
                    "pending receives to proceed tx: {:?}",
                    tx_meta.meta.uuid
                )));
            }
        }

        // Here tx can be incorporated

        sequence.push(Action::Receive(receives));
        sequence.push(Action::Tx(tx_meta.clone(), Box::new(tx_data.clone())));
    }

    // Finally, take all deposits and transfers
    let receives = collect_receives(&None, &mut deposits, &mut transfers).await?;
    sequence.push(Action::Receive(receives));

    let pending_deposit_uuids = deposit_info
        .pending
        .iter()
        .map(|(meta, _)| meta.uuid.clone())
        .collect();
    let pending_transfer_uuids = transfer_info
        .pending
        .iter()
        .map(|(meta, _)| meta.uuid.clone())
        .collect();

    Ok((
        sequence,
        PendingInfo {
            pending_deposit_uuids,
            pending_transfer_uuids,
        },
    ))
}

/// For each settled tx, take deposits and transfers that are strictly smaller than the block number of the tx
/// If there is no tx, take all deposit and transfer data
async fn collect_receives(
    tx: &Option<(MetaDataWithBlockNumber, TxData)>,
    deposits: &mut Vec<(MetaDataWithBlockNumber, DepositData)>,
    transfers: &mut Vec<(MetaDataWithBlockNumber, TransferData)>,
) -> Result<Vec<ReceiveAction>, StrategyError> {
    let mut receives: Vec<ReceiveAction> = Vec::new();
    if let Some((meta, _tx_data)) = tx {
        let block_number = meta.block_number;

        // take and remove deposit that are strictly smaller than the block number of the tx
        let receive_deposit = deposits
            .iter()
            .filter(|(meta, _)| meta.block_number < block_number)
            .map(|(meta, data)| ReceiveAction::Deposit(meta.clone(), data.clone()))
            .collect_vec();
        deposits.retain(|(meta, _)| meta.block_number >= block_number);

        // take and remove transfer that are strictly smaller than the block number of the tx
        let receive_transfer = transfers
            .iter()
            .filter(|(meta, _)| meta.block_number < block_number)
            .map(|(meta, data)| ReceiveAction::Transfer(meta.clone(), Box::new(data.clone())))
            .collect_vec();
        transfers.retain(|(meta, _)| meta.block_number >= block_number);

        // add to receives
        receives.extend(receive_deposit);
        receives.extend(receive_transfer);
    } else {
        // if there is no tx, take all deposit and transfer data
        let receive_deposit = deposits
            .iter()
            .map(|(meta, data)| ReceiveAction::Deposit(meta.clone(), data.clone()))
            .collect_vec();
        deposits.clear();

        let receive_transfer = transfers
            .iter()
            .map(|(meta, data)| ReceiveAction::Transfer(meta.clone(), Box::new(data.clone())))
            .collect_vec();
        transfers.clear();

        receives.extend(receive_deposit);
        receives.extend(receive_transfer);
    }

    // sort by block number first, then by uuid to make the order deterministic
    receives.sort_by_key(|action| {
        let meta = action.meta();
        (meta.block_number, meta.meta.uuid.clone())
    });

    Ok(receives)
}

/// Determine the sequence of withdrawal tx
pub async fn determine_withdrawals<
    S: StoreVaultClientInterface,
    V: ValidityProverClientInterface,
>(
    store_vault_server: &S,
    validity_prover: &V,
    key: KeySet,
    tx_timeout: u64,
) -> Result<
    (
        Vec<(MetaDataWithBlockNumber, TransferData)>,
        Vec<String>, // pending withdrawals
    ),
    StrategyError,
> {
    log::info!("determine_withdrawals");
    let user_data = fetch_user_data(store_vault_server, key).await?;
    let withdrawal_info = fetch_all_unprocessed_withdrawal_info(
        store_vault_server,
        validity_prover,
        key,
        &user_data.withdrawal_status,
        tx_timeout,
    )
    .await?;
    let pending_withdrawal_uuids = withdrawal_info
        .pending
        .iter()
        .map(|(meta, _)| meta.uuid.clone())
        .collect();
    Ok((withdrawal_info.settled, pending_withdrawal_uuids))
}

/// Determine the
pub async fn determine_claims<S: StoreVaultClientInterface, V: ValidityProverClientInterface>(
    store_vault_server: &S,
    validity_prover: &V,
    liquidity_contract: &LiquidityContract,
    key: KeySet,
    tx_timeout: u64,
    deposit_timeout: u64,
) -> Result<Vec<Mining>, StrategyError> {
    log::info!("determine_claims");
    let user_data = fetch_user_data(store_vault_server, key).await?;
    let minings = fetch_mining_info(
        store_vault_server,
        validity_prover,
        liquidity_contract,
        key,
        &user_data.claim_status,
        tx_timeout,
        deposit_timeout,
    )
    .await?;
    // pickup the largest deposit
    let claims = minings
        .into_iter()
        .filter(|mining| matches!(mining.status, MiningStatus::Claimable(_)))
        .collect();
    Ok(claims)
}
