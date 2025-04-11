use async_trait::async_trait;
use intmax2_interfaces::api::{
    balance_prover::{
        interface::BalanceProverClientInterface,
        types::{
            ProveReceiveDepositRequest, ProveReceiveTransferRequest, ProveResponse,
            ProveSendRequest, ProveSingleClaimRequest, ProveSingleWithdrawalRequest,
            ProveSpentRequest, ProveUpdateRequest,
        },
    },
    error::ServerError,
};
use intmax2_zkp::{
    common::{
        signature_content::key_set::KeySet,
        witness::{
            claim_witness::ClaimWitness, receive_deposit_witness::ReceiveDepositWitness,
            receive_transfer_witness::ReceiveTransferWitness, spent_witness::SpentWitness,
            tx_witness::TxWitness, update_witness::UpdateWitness,
            withdrawal_witness::WithdrawalWitness,
        },
    },
    ethereum_types::u256::U256,
};
use plonky2::{
    field::goldilocks_field::GoldilocksField,
    plonk::{config::PoseidonGoldilocksConfig, proof::ProofWithPublicInputs},
};

use super::utils::query::post_request;

type F = GoldilocksField;
type C = PoseidonGoldilocksConfig;
const D: usize = 2;

#[derive(Debug, Clone)]
pub struct BalanceProverClient {
    base_url: String,
}

impl BalanceProverClient {
    pub fn new(base_url: &str) -> Self {
        BalanceProverClient {
            base_url: base_url.to_string(),
        }
    }
}

#[async_trait(?Send)]
impl BalanceProverClientInterface for BalanceProverClient {
    async fn prove_spent(
        &self,
        _key: KeySet,
        spent_witness: &SpentWitness,
    ) -> Result<ProofWithPublicInputs<F, C, D>, ServerError> {
        let request = ProveSpentRequest {
            spent_witness: spent_witness.clone(),
        };
        let response: ProveResponse = post_request(
            &self.base_url,
            "/balance-prover/prove-spent",
            Some(&request),
        )
        .await?;
        Ok(response.proof)
    }

    async fn prove_send(
        &self,
        _key: KeySet,
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
            post_request(&self.base_url, "/balance-prover/prove-send", Some(&request)).await?;
        Ok(response.proof)
    }

    async fn prove_update(
        &self,
        _key: KeySet,
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
            &self.base_url,
            "/balance-prover/prove-update",
            Some(&request),
        )
        .await?;
        Ok(response.proof)
    }

    async fn prove_receive_transfer(
        &self,
        _key: KeySet,
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
            &self.base_url,
            "/balance-prover/prove-receive-transfer",
            Some(&request),
        )
        .await?;
        Ok(response.proof)
    }

    async fn prove_receive_deposit(
        &self,
        _key: KeySet,
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
            &self.base_url,
            "/balance-prover/prove-receive-deposit",
            Some(&request),
        )
        .await?;
        Ok(response.proof)
    }

    async fn prove_single_withdrawal(
        &self,
        _key: KeySet,
        withdrawal_witness: &WithdrawalWitness<F, C, D>,
    ) -> Result<ProofWithPublicInputs<F, C, D>, ServerError> {
        let request = ProveSingleWithdrawalRequest {
            withdrawal_witness: withdrawal_witness.clone(),
        };
        let response: ProveResponse = post_request(
            &self.base_url,
            "/balance-prover/prove-single-withdrawal",
            Some(&request),
        )
        .await?;
        Ok(response.proof)
    }

    async fn prove_single_claim(
        &self,
        _key: KeySet,
        is_faster_mining: bool,
        claim_witness: &ClaimWitness<F, C, D>,
    ) -> Result<ProofWithPublicInputs<F, C, D>, ServerError> {
        let request = ProveSingleClaimRequest {
            is_faster_mining,
            claim_witness: claim_witness.clone(),
        };
        let response: ProveResponse = post_request(
            &self.base_url,
            "/balance-prover/prove-single-claim",
            Some(&request),
        )
        .await?;
        Ok(response.proof)
    }
}
