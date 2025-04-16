use std::fmt::{self, Display, Formatter};

use async_trait::async_trait;
use intmax2_zkp::{
    common::{claim::Claim, signature_content::key_set::KeySet},
    ethereum_types::{address::Address, bytes32::Bytes32, u256::U256, u32limb_trait::U32LimbTrait},
};
use plonky2::{
    field::goldilocks_field::GoldilocksField,
    plonk::{config::PoseidonGoldilocksConfig, proof::ProofWithPublicInputs},
};
use plonky2_keccak::utils::solidity_keccak256;
use serde::{Deserialize, Serialize};

use crate::api::{block_builder::interface::Fee, error::ServerError};

type F = GoldilocksField;
type C = PoseidonGoldilocksConfig;
const D: usize = 2;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WithdrawalFeeInfo {
    pub beneficiary: Option<U256>,
    pub direct_withdrawal_fee: Option<Vec<Fee>>,
    pub claimable_withdrawal_fee: Option<Vec<Fee>>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimFeeInfo {
    pub beneficiary: Option<U256>,
    pub fee: Option<Vec<Fee>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WithdrawalInfo {
    pub status: WithdrawalStatus,
    pub contract_withdrawal: ContractWithdrawal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimInfo {
    pub status: ClaimStatus,
    pub claim: Claim,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContractWithdrawal {
    pub recipient: Address,
    pub token_index: u32,
    pub amount: U256,
    pub nullifier: Bytes32,
}

impl ContractWithdrawal {
    pub fn withdrawal_hash(&self) -> Bytes32 {
        let mut input = Vec::new();
        input.extend_from_slice(&self.recipient.to_u32_vec());
        input.extend_from_slice(&[self.token_index]);
        input.extend_from_slice(&self.amount.to_u32_vec());
        input.extend_from_slice(&self.nullifier.to_u32_vec());
        Bytes32::from_u32_slice(solidity_keccak256(&input).as_slice()).unwrap()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum ClaimStatus {
    Requested = 0,
    Verified = 1,
    Relayed = 2,
    Success = 3,
    Failed = 4, // Should be never used but just in case
}

impl Display for ClaimStatus {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            ClaimStatus::Requested => write!(f, "requested"),
            ClaimStatus::Verified => write!(f, "verified"),
            ClaimStatus::Relayed => write!(f, "relayed"),
            ClaimStatus::Success => write!(f, "success"),
            ClaimStatus::Failed => write!(f, "failed"),
        }
    }
}

#[async_trait(?Send)]
pub trait WithdrawalServerClientInterface: Sync + Send {
    async fn get_withdrawal_fee(&self) -> Result<WithdrawalFeeInfo, ServerError>;

    async fn get_claim_fee(&self) -> Result<ClaimFeeInfo, ServerError>;

    async fn request_withdrawal(
        &self,
        key: KeySet,
        single_withdrawal_proof: &ProofWithPublicInputs<F, C, D>,
        fee_token_index: Option<u32>,
        fee_transfer_digests: &[Bytes32],
    ) -> Result<(), ServerError>;

    async fn request_claim(
        &self,
        key: KeySet,
        single_claim_proof: &ProofWithPublicInputs<F, C, D>,
        fee_token_index: Option<u32>,
        fee_transfer_digests: &[Bytes32],
    ) -> Result<(), ServerError>;

    async fn get_withdrawal_info(&self, key: KeySet) -> Result<Vec<WithdrawalInfo>, ServerError>;

    async fn get_withdrawal_info_by_recipient(
        &self,
        recipient: Address,
    ) -> Result<Vec<WithdrawalInfo>, ServerError>;

    async fn get_claim_info(&self, key: KeySet) -> Result<Vec<ClaimInfo>, ServerError>;
}
