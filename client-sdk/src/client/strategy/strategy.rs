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

use intmax2_zkp::{
    common::signature::key_set::KeySet,
    ethereum_types::{bytes32::Bytes32, u32limb_trait::U32LimbTrait},
};

use super::{error::StrategyError, mining::Mining};
use crate::{
    client::strategy::{
        common::fetch_user_data,
        deposit::fetch_all_unprocessed_deposit_info,
        mining::{fetch_mining_info, MiningStatus},
        transfer::fetch_all_unprocessed_transfer_info,
        tx::fetch_all_unprocessed_tx_info,
        tx_status::{get_tx_status, TxStatus},
        utils::wait_till_validity_prover_synced,
        withdrawal::fetch_all_unprocessed_withdrawal_info,
    },
    external_api::contract::{
        liquidity_contract::LiquidityContract, rollup_contract::RollupContract,
    },
};

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
    pub pending_deposit_digests: Vec<Bytes32>,
    pub pending_transfer_digests: Vec<Bytes32>,
}

/// Determine the sequence of receives/send tx to be incorporated into the balance proof
pub async fn determine_sequence(
    store_vault_server: &dyn StoreVaultClientInterface,
    validity_prover: &dyn ValidityProverClientInterface,
    rollup_contract: &RollupContract,
    liquidity_contract: &LiquidityContract,
    key: KeySet,
    deposit_timeout: u64,
    tx_timeout: u64,
) -> Result<(Vec<Action>, PendingInfo), StrategyError> {
    log::info!("determine_sequence");

    // Wait until the validity prover catches up with the onchain block number
    let current_time = chrono::Utc::now().timestamp() as u64;
    let onchain_block_number = rollup_contract.get_latest_block_number().await?;
    wait_till_validity_prover_synced(validity_prover, false, onchain_block_number).await?;

    let user_data = fetch_user_data(store_vault_server, key).await?;
    let mut nonce = user_data.full_private_state.nonce;
    let mut balances = user_data.balances();
    if balances.is_insufficient() {
        return Err(StrategyError::BalanceInsufficientBeforeSync);
    }

    let tx_info = fetch_all_unprocessed_tx_info(
        store_vault_server,
        validity_prover,
        key,
        current_time,
        &user_data.tx_status,
        tx_timeout,
    )
    .await?;

    //  First, if there is a pending tx, return a pending error
    if let Some((meta, _tx_data)) = tx_info.pending.first() {
        return Err(StrategyError::PendingTxError(format!(
            "pending tx: {:?}",
            meta.digest
        )));
    }

    // Then, collect deposit and transfer data
    let deposit_info = fetch_all_unprocessed_deposit_info(
        store_vault_server,
        validity_prover,
        liquidity_contract,
        key,
        current_time,
        &user_data.deposit_status,
        deposit_timeout,
    )
    .await?;
    let transfer_info = fetch_all_unprocessed_transfer_info(
        store_vault_server,
        validity_prover,
        key,
        current_time,
        &user_data.transfer_status,
        tx_timeout,
    )
    .await?;
    log::info!(
        "num of deposits: pending={}, settled={}",
        deposit_info.pending.len(),
        deposit_info.settled.len()
    );
    log::info!(
        "num of transfers: pending={}, settled={}",
        transfer_info.pending.len(),
        transfer_info.settled.len()
    );
    log::info!(
        "num of txs: pending={}, settled={}",
        tx_info.pending.len(),
        tx_info.settled.len()
    );

    let mut deposits = deposit_info.settled;
    let mut transfers = transfer_info.settled;

    // Next, for each settled tx, take deposits and transfers that are strictly smaller than the block number of the tx
    let mut sequence = Vec::new();
    for (tx_meta, tx_data) in tx_info.settled.iter() {
        // validate tx status
        let tx_status = get_tx_status(validity_prover, key.pubkey, tx_data.tx_tree_root).await?;
        if tx_status != TxStatus::Success {
            log::warn!("tx {} is not success: {}", tx_meta.meta.digest, tx_status);
            continue;
        }

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
        let is_insufficient = if tx_data.spent_witness.tx.nonce == nonce {
            nonce += 1;
            balances.sub_tx(tx_data)
        } else {
            // ignore nonce mismatch tx
            log::warn!(
                "nonce mismatch tx {}: expected={}, actual={}",
                tx_meta.meta.digest,
                nonce,
                tx_data.spent_witness.tx.nonce
            );
            false
        };

        if is_insufficient {
            if deposit_info.pending.is_empty() && transfer_info.pending.is_empty() {
                // Unresolved balance shortage
                return Err(StrategyError::BalanceInsufficientDuringSync);
            } else {
                // To incorporate the tx, you need to incorporate the pending deposit/transfer to solve the balance shortage.
                // TODO: Processing when the balance shortage is not resolved even if the pending deposit/transfer is incorporated
                return Err(StrategyError::PendingReceivesError(format!(
                    "pending receives to proceed tx: {:?}",
                    tx_meta.meta.digest
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

    let pending_deposit_digests = deposit_info
        .pending
        .iter()
        .map(|(meta, _)| meta.digest)
        .collect();
    let pending_transfer_digests = transfer_info
        .pending
        .iter()
        .map(|(meta, _)| meta.digest)
        .collect();

    Ok((
        sequence,
        PendingInfo {
            pending_deposit_digests,
            pending_transfer_digests,
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
        (meta.block_number, meta.meta.digest.to_hex())
    });

    Ok(receives)
}

/// Determine the sequence of withdrawal tx
pub async fn determine_withdrawals(
    store_vault_server: &dyn StoreVaultClientInterface,
    validity_prover: &dyn ValidityProverClientInterface,
    rollup_contract: &RollupContract,
    key: KeySet,
    tx_timeout: u64,
) -> Result<
    (
        Vec<(MetaDataWithBlockNumber, TransferData)>,
        Vec<Bytes32>, // pending withdrawals
    ),
    StrategyError,
> {
    log::info!("determine_withdrawals");

    // Wait until the validity prover catches up with the onchain block number
    let current_time = chrono::Utc::now().timestamp() as u64;
    let onchain_block_number = rollup_contract.get_latest_block_number().await?;
    wait_till_validity_prover_synced(validity_prover, false, onchain_block_number).await?;

    let user_data = fetch_user_data(store_vault_server, key).await?;
    let withdrawal_info = fetch_all_unprocessed_withdrawal_info(
        store_vault_server,
        validity_prover,
        key,
        current_time,
        &user_data.withdrawal_status,
        tx_timeout,
    )
    .await?;
    let pending_withdrawal_digests = withdrawal_info
        .pending
        .iter()
        .map(|(meta, _)| meta.digest)
        .collect();
    Ok((withdrawal_info.settled, pending_withdrawal_digests))
}

/// Determine the
pub async fn determine_claims(
    store_vault_server: &dyn StoreVaultClientInterface,
    validity_prover: &dyn ValidityProverClientInterface,
    rollup_contract: &RollupContract,
    liquidity_contract: &LiquidityContract,
    key: KeySet,
    tx_timeout: u64,
    deposit_timeout: u64,
) -> Result<Vec<Mining>, StrategyError> {
    log::info!("determine_claims");

    // Wait until the validity prover catches up with the onchain block number
    let current_time = chrono::Utc::now().timestamp() as u64;
    let onchain_block_number = rollup_contract.get_latest_block_number().await?;
    wait_till_validity_prover_synced(validity_prover, false, onchain_block_number).await?;

    let user_data = fetch_user_data(store_vault_server, key).await?;
    let minings = fetch_mining_info(
        store_vault_server,
        validity_prover,
        liquidity_contract,
        key,
        current_time,
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
