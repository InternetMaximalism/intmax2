use std::fmt::{self, Display, Formatter};

use async_trait::async_trait;
use intmax2_zkp::{
    common::{signature::key_set::KeySet, withdrawal::Withdrawal},
    ethereum_types::{address::Address, u256::U256},
};
use plonky2::{
    field::goldilocks_field::GoldilocksField,
    plonk::{config::PoseidonGoldilocksConfig, proof::ProofWithPublicInputs},
};
use serde::{Deserialize, Serialize};

use crate::api::error::ServerError;

type F = GoldilocksField;
type C = PoseidonGoldilocksConfig;
const D: usize = 2;

/// fee = constant + coefficient * amount
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Fee {
    pub token_index: u32,
    pub constant: u128,
    pub coefficient: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WithdrawalInfo {
    pub status: WithdrawalStatus,
    pub withdrawal: Withdrawal,
    pub withdrawal_id: Option<u32>,
}

impl WithdrawalInfo {
    pub fn to_contract_withdrawal(&self) -> Option<ContractWithdrawal> {
        self.withdrawal_id.map(|id| ContractWithdrawal {
            recipient: self.withdrawal.recipient,
            token_index: self.withdrawal.token_index,
            amount: self.withdrawal.amount,
            id,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContractWithdrawal {
    pub recipient: Address,
    pub token_index: u32,
    pub amount: U256,
    pub id: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WithdrawalStatus {
    Requested = 0,
    Relayed = 1,
    Success = 2,
    NeedClaim = 3,
    Failed = 4, // Should be never used but just in case
}

impl Display for WithdrawalStatus {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            WithdrawalStatus::Requested => write!(f, "requested"),
            WithdrawalStatus::Relayed => write!(f, "relayed"),
            WithdrawalStatus::Success => write!(f, "success"),
            WithdrawalStatus::NeedClaim => write!(f, "need_claim"),
            WithdrawalStatus::Failed => write!(f, "failed"),
        }
    }
}

#[async_trait(?Send)]
pub trait WithdrawalServerClientInterface {
    async fn fee(&self) -> Result<Vec<Fee>, ServerError>;

    async fn request_withdrawal(
        &self,
        pubkey: U256,
        single_withdrawal_proof: &ProofWithPublicInputs<F, C, D>,
    ) -> Result<(), ServerError>;

    async fn get_withdrawal_info(&self, key: KeySet) -> Result<Vec<WithdrawalInfo>, ServerError>;

    async fn get_withdrawal_info_by_recipient(
        &self,
        recipient: Address,
    ) -> Result<Vec<WithdrawalInfo>, ServerError>;
}
