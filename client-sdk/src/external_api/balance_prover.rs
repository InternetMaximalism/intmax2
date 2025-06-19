use async_trait::async_trait;
use intmax2_interfaces::{
    api::{
        balance_prover::{
            interface::BalanceProverClientInterface,
            types::{
                ProveReceiveDepositRequest, ProveReceiveTransferRequest, ProveResponse,
                ProveSendRequest, ProveSingleClaimRequest, ProveSingleWithdrawalRequest,
                ProveSpentRequest, ProveUpdateRequest,
            },
        },
        error::ServerError,
    },
    utils::key::PrivateKey,
};
use intmax2_zkp::{
    common::witness::{
        claim_witness::ClaimWitness, receive_deposit_witness::ReceiveDepositWitness,
        receive_transfer_witness::ReceiveTransferWitness, spent_witness::SpentWitness,
        tx_witness::TxWitness, update_witness::UpdateWitness,
        withdrawal_witness::WithdrawalWitness,
    },
    ethereum_types::u256::U256,
};
use plonky2::{
    field::goldilocks_field::GoldilocksField,
    plonk::{config::PoseidonGoldilocksConfig, proof::ProofWithPublicInputs},
};
use reqwest::Client;

use crate::external_api::utils::query::build_client;

use super::utils::query::post_request;

type F = GoldilocksField;
type C = PoseidonGoldilocksConfig;
const D: usize = 2;

#[derive(Debug, Clone)]
pub struct BalanceProverClient {
    client: Client,
    base_url: String,
}

impl BalanceProverClient {
    pub fn new(base_url: &str) -> Self {
        BalanceProverClient {
            client: build_client(),
            base_url: base_url.to_string(),
        }
    }
}

#[async_trait(?Send)]
impl BalanceProverClientInterface for BalanceProverClient {
    async fn prove_spent(
        &self,
        _view_key: PrivateKey,
        spent_witness: &SpentWitness,
    ) -> Result<ProofWithPublicInputs<F, C, D>, ServerError> {
        let request = ProveSpentRequest {
            spent_witness: spent_witness.clone(),
        };
        let response: ProveResponse =
            post_request(&self.client, &self.base_url, "/prove-spent", Some(&request)).await?;
        Ok(response.proof)
    }

    async fn prove_send(
        &self,
        _view_key: PrivateKey,
        pubkey: U256,
        tx_witness: &TxWitness,
        update_witness: &UpdateWitness<F, C, D>,
        spent_proof: &ProofWithPublicInputs<F, C, D>,
        prev_proof: &Option<ProofWithPublicInputs<F, C, D>>,
    ) -> Result<ProofWithPublicInputs<F, C, D>, ServerError> {
        let request = ProveSendRequest {
            pubkey,
            tx_witness: tx_witness.clone(),
            update_witness: update_witness.clone(),
            spent_proof: spent_proof.clone(),
            prev_proof: prev_proof.clone(),
        };
        let response: ProveResponse =
            post_request(&self.client, &self.base_url, "/prove-send", Some(&request)).await?;
        Ok(response.proof)
    }

    async fn prove_update(
        &self,
        _view_key: PrivateKey,
        pubkey: U256,
        update_witness: &UpdateWitness<F, C, D>,
        prev_proof: &Option<ProofWithPublicInputs<F, C, D>>,
    ) -> Result<ProofWithPublicInputs<F, C, D>, ServerError> {
        let request = ProveUpdateRequest {
            pubkey,
            update_witness: update_witness.clone(),
            prev_proof: prev_proof.clone(),
        };
        let response: ProveResponse = post_request(
            &self.client,
            &self.base_url,
            "/prove-update",
            Some(&request),
        )
        .await?;
        Ok(response.proof)
    }

    async fn prove_receive_transfer(
        &self,
        _view_key: PrivateKey,
        pubkey: U256,
        receive_transfer_witness: &ReceiveTransferWitness<F, C, D>,
        prev_proof: &Option<ProofWithPublicInputs<F, C, D>>,
    ) -> Result<ProofWithPublicInputs<F, C, D>, ServerError> {
        let request = ProveReceiveTransferRequest {
            pubkey,
            receive_transfer_witness: receive_transfer_witness.clone(),
            prev_proof: prev_proof.clone(),
        };
        let response: ProveResponse = post_request(
            &self.client,
            &self.base_url,
            "/prove-receive-transfer",
            Some(&request),
        )
        .await?;
        Ok(response.proof)
    }

    async fn prove_receive_deposit(
        &self,
        _view_key: PrivateKey,
        pubkey: U256,
        receive_deposit_witness: &ReceiveDepositWitness,
        prev_proof: &Option<ProofWithPublicInputs<F, C, D>>,
    ) -> Result<ProofWithPublicInputs<F, C, D>, ServerError> {
        let request = ProveReceiveDepositRequest {
            pubkey,
            receive_deposit_witness: receive_deposit_witness.clone(),
            prev_proof: prev_proof.clone(),
        };
        let response: ProveResponse = post_request(
            &self.client,
            &self.base_url,
            "/prove-receive-deposit",
            Some(&request),
        )
        .await?;
        Ok(response.proof)
    }

    async fn prove_single_withdrawal(
        &self,
        _view_key: PrivateKey,
        withdrawal_witness: &WithdrawalWitness<F, C, D>,
    ) -> Result<ProofWithPublicInputs<F, C, D>, ServerError> {
        let request = ProveSingleWithdrawalRequest {
            withdrawal_witness: withdrawal_witness.clone(),
        };
        let response: ProveResponse = post_request(
            &self.client,
            &self.base_url,
            "/prove-single-withdrawal",
            Some(&request),
        )
        .await?;
        Ok(response.proof)
    }

    async fn prove_single_claim(
        &self,
        _view_key: PrivateKey,
        is_faster_mining: bool,
        claim_witness: &ClaimWitness<F, C, D>,
    ) -> Result<ProofWithPublicInputs<F, C, D>, ServerError> {
        let request = ProveSingleClaimRequest {
            is_faster_mining,
            claim_witness: claim_witness.clone(),
        };
        let response: ProveResponse = post_request(
            &self.client,
            &self.base_url,
            "/prove-single-claim",
            Some(&request),
        )
        .await?;
        Ok(response.proof)
    }
}
