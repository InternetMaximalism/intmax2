use async_trait::async_trait;
use intmax2_zkp::{
    common::{
        block_builder::BlockProposal, signature_content::flatten::FlatG2, tx::Tx,
        witness::transfer_witness::TransferWitness,
    },
    ethereum_types::{address::Address, u256::U256},
};
use serde::{Deserialize, Serialize};

use crate::{
    api::error::ServerError,
    data::transfer_data::TransferData,
    utils::{address::IntmaxAddress, fee::Fee, key::PrivateKey},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeeProof {
    pub sender_proof_set_ephemeral_key: PrivateKey,
    pub fee_transfer_witness: TransferWitness,
    pub collateral_block: Option<CollateralBlock>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollateralBlock {
    pub sender_proof_set_ephemeral_key: PrivateKey,
    pub fee_transfer_data: TransferData,
    pub is_registration_block: bool,
    pub expiry: u64,
    pub block_builder_address: Address,
    pub signature: FlatG2,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BlockBuilderStatus {
    Pausing,        // not accepting tx requests
    AcceptingTxs,   // accepting  tx request
    ProposingBlock, // after constructed the block, accepting signatures for the block
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockBuilderFeeInfo {
    pub version: String,
    pub block_builder_address: Address,
    pub beneficiary: IntmaxAddress,
    pub registration_fee: Option<Vec<Fee>>,
    pub non_registration_fee: Option<Vec<Fee>>,
    pub registration_collateral_fee: Option<Vec<Fee>>,
    pub non_registration_collateral_fee: Option<Vec<Fee>>,
}

#[async_trait(?Send)]
pub trait BlockBuilderClientInterface: Sync + Send {
    async fn get_fee_info(
        &self,
        block_builder_url: &str,
    ) -> Result<BlockBuilderFeeInfo, ServerError>;

    // Send tx request to the block builder
    async fn send_tx_request(
        &self,
        block_builder_url: &str,
        is_registration_block: bool,
        sender: IntmaxAddress,
        tx: Tx,
        fee_proof: Option<FeeProof>,
    ) -> Result<String, ServerError>;

    // Query tx tree root proposal from the block builder
    async fn query_proposal(
        &self,
        block_builder_url: &str,
        request_id: &str,
    ) -> Result<Option<BlockProposal>, ServerError>;

    // Send signature to the block builder
    async fn post_signature(
        &self,
        block_builder_url: &str,
        request_id: &str,
        pubkey: U256,
        signature: FlatG2,
    ) -> Result<(), ServerError>;
}
