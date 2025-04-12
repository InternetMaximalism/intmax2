use std::fmt;

use intmax2_interfaces::api::validity_prover::interface::ValidityProverClientInterface;
use intmax2_zkp::ethereum_types::{bytes32::Bytes32, u256::U256};
use serde::{Deserialize, Serialize};

use super::error::StrategyError;

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum TxStatus {
    Pending,
    Success,
    Failed(String),
}

impl fmt::Display for TxStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TxStatus::Pending => write!(f, "pending"),
            TxStatus::Success => write!(f, "success"),
            TxStatus::Failed(_) => write!(f, "failed"),
        }
    }
}

pub async fn get_tx_status(
    validity_prover: &dyn ValidityProverClientInterface,
    sender: U256,
    tx_tree_root: Bytes32,
) -> Result<TxStatus, StrategyError> {
    // get onchain info
    let block_number = validity_prover
        .get_block_number_by_tx_tree_root(tx_tree_root)
        .await?;
    if block_number.is_none() {
        return Ok(TxStatus::Pending);
    }
    let block_number = block_number.unwrap();
    let validity_witness = validity_prover.get_validity_witness(block_number).await?;
    let validity_pis = validity_witness.to_validity_pis().map_err(|e| {
        StrategyError::UnexpectedError(format!("failed to convert to validity pis: {}", e))
    })?;

    // get sender leaf
    let sender_leaf = validity_witness
        .block_witness
        .get_sender_tree()
        .leaves()
        .into_iter()
        .find(|leaf| leaf.sender == sender);
    let sender_leaf = match sender_leaf {
        Some(leaf) => leaf,
        None => return Ok(TxStatus::Failed("sender leaf not found".to_string())),
    };

    if !sender_leaf.signature_included {
        return Ok(TxStatus::Failed(
            "sender did'nt returned signature".to_string(),
        ));
    }

    if !validity_pis.is_valid_block {
        return Ok(TxStatus::Failed("block is not valid".to_string()));
    }

    Ok(TxStatus::Success)
}
