use intmax2_interfaces::{
    api::{
        store_vault_server::interface::StoreVaultClientInterface,
        validity_prover::interface::ValidityProverClientInterface,
    },
    data::{
        deposit_data::{DepositData, TokenType},
        meta_data::MetaDataWithBlockNumber,
        user_data::ProcessStatus,
    },
};
use intmax2_zkp::{
    circuits::claim::determine_lock_time::get_lock_time,
    common::{block::Block, signature::key_set::KeySet},
    ethereum_types::u256::U256,
};
use num_bigint::BigUint;
use std::fmt::Display;

use crate::external_api::contract::liquidity_contract::LiquidityContract;

use super::{
    deposit::fetch_all_unprocessed_deposit_info, error::StrategyError,
    tx::fetch_all_unprocessed_tx_info,
};

#[derive(Debug, Clone)]
pub struct Mining {
    pub meta: MetaDataWithBlockNumber,
    pub deposit_data: DepositData,
    pub block: Block,  // the first block that contains the deposit
    pub maturity: u64, // maturity unix timestamp
    pub status: MiningStatus,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MiningStatus {
    Disqualified,   // Disqualified because there is a send tx before the maturity
    Locking,        // In locking period
    Claimable(u32), // Claimable with the block number at the time of claim
}

impl Display for MiningStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MiningStatus::Disqualified => write!(f, "Disqualified"),
            MiningStatus::Locking => write!(f, "Locking"),
            MiningStatus::Claimable(block_number) => {
                write!(f, "Claimable at block {}", block_number)
            }
        }
    }
}

pub async fn fetch_mining_info(
    store_vault_server: &dyn StoreVaultClientInterface,
    validity_prover: &dyn ValidityProverClientInterface,
    liquidity_contract: &LiquidityContract,
    key: KeySet,
    claim_status: &ProcessStatus,
    tx_timeout: u64,
    deposit_timeout: u64,
) -> Result<Vec<Mining>, StrategyError> {
    // get all deposit info
    let deposit_info = fetch_all_unprocessed_deposit_info(
        store_vault_server,
        validity_prover,
        liquidity_contract,
        key,
        &ProcessStatus::default(),
        deposit_timeout,
    )
    .await?;
    // filter out ineligible deposits
    let candidate_deposits = deposit_info
        .settled
        .into_iter()
        .filter(|(meta, deposit_data)| {
            let deposit = deposit_data.deposit().unwrap(); // unwrap is safe here because already settled
            if !deposit.is_eligible {
                // skip ineligible deposits
                return false;
            }
            if !validate_mining_deposit_criteria(deposit_data.token_type, deposit.amount) {
                // skip deposits that do not meet the mining criteria
                return false;
            }
            if claim_status.processed_uuids.contains(&meta.meta.uuid) {
                // skip deposits that are already claimed
                return false;
            }
            true
        })
        .collect::<Vec<_>>();
    if candidate_deposits.is_empty() {
        // early return if no eligible deposits
        return Ok(vec![]);
    }

    // fetch last block number
    let account_info = validity_prover.get_account_info(key.pubkey).await?;
    let last_block_number = account_info.last_block_number;

    // get tx info
    let tx_info = fetch_all_unprocessed_tx_info(
        store_vault_server,
        validity_prover,
        key,
        &ProcessStatus::default(),
        tx_timeout,
    )
    .await?;
    let settled_txs = tx_info.settled;

    let mut minings = Vec::new();
    let current_block_number = validity_prover.get_block_number().await?;
    let current_block = fetch_block(validity_prover, current_block_number).await?;
    let current_time = current_block.timestamp;

    for (meta, deposit_data) in candidate_deposits {
        let block = fetch_block(validity_prover, meta.block_number).await?;
        let lock_time = get_lock_time(block.hash(), deposit_data.deposit_salt);
        let maturity = block.timestamp + lock_time;
        let status = {
            if block.block_number <= last_block_number {
                // there is a send tx after the deposit
                // get the first send tx block number
                let (meta, _) = settled_txs
                    .iter()
                    .filter(|(meta, _)| meta.block_number > block.block_number)
                    .min_by_key(|(meta, _)| meta.block_number)
                    .expect("send tx block number not found"); // must exist because there is a send tx after the deposit
                                                               // one block before tx is the candidate of the claimable block number
                let candidate_claimable_block_number = meta.block_number - 1;
                let candidate_claimable_block =
                    fetch_block(validity_prover, candidate_claimable_block_number).await?;
                if candidate_claimable_block.timestamp < maturity {
                    // the send tx is before the maturity
                    MiningStatus::Disqualified
                } else {
                    // the send tx is after the maturity
                    MiningStatus::Claimable(candidate_claimable_block_number)
                }
            } else if current_time < maturity {
                // in locking period
                MiningStatus::Locking
            } else {
                // claimable now
                MiningStatus::Claimable(current_block_number)
            }
        };
        minings.push(Mining {
            meta,
            deposit_data,
            block,
            maturity,
            status,
        });
    }
    Ok(minings)
}

pub fn validate_mining_deposit_criteria(token_type: TokenType, amount: U256) -> bool {
    if token_type != TokenType::NATIVE {
        return false;
    }
    // O.1 ETH, 1 ETH, 10 ETH, 100 ETH
    let candidates: Vec<BigUint> = (0..4).map(|i| BigUint::from(10u32).pow(i + 17)).collect();
    let amount: BigUint = amount.into();
    candidates.contains(&amount)
}

async fn fetch_block(
    validity_prover: &dyn ValidityProverClientInterface,
    block_number: u32,
) -> Result<Block, StrategyError> {
    let validity_witness = validity_prover.get_validity_witness(block_number).await?;
    Ok(validity_witness.block_witness.block)
}
