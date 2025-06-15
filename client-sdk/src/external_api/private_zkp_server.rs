use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use base64::{prelude::BASE64_STANDARD, Engine};
use intmax2_interfaces::{
    api::{
        balance_prover::{
            interface::BalanceProverClientInterface,
            types::{
                ProveReceiveDepositRequest, ProveReceiveTransferRequest, ProveSendRequest,
                ProveSingleClaimRequest, ProveSingleWithdrawalRequest, ProveSpentRequest,
                ProveUpdateRequest,
            },
        },
        error::ServerError,
        private_zkp_server::types::{
            CreateProofResponse, CreateProveRequest, GetPublicKeyResponse, ProofResultQuery,
            ProofResultResponse, ProofResultWithError, ProveRequestWithType, ProveType,
        },
    },
    data::encryption::{BlsEncryption, RsaEncryption},
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
use rsa::{pkcs8::DecodePublicKey, RsaPublicKey};
use serde::{Deserialize, Serialize};

use crate::external_api::utils::{query::build_client, time::sleep_for};

use super::utils::query::{get_request, post_request};

type F = GoldilocksField;
type C = PoseidonGoldilocksConfig;
const D: usize = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivateZKPServerConfig {
    pub max_retries: usize,
    pub retry_interval: u64,
}

#[derive(Debug, Clone)]
pub struct PrivateZKPServerClient {
    client: Client,

    base_url: String,

    config: PrivateZKPServerConfig,

    // rsa public key is used to encrypt the prove request
    // because async OnceLock is not stable, we use RwLock + Option instead
    pubkey: Arc<RwLock<Option<RsaPublicKey>>>,
}

impl PrivateZKPServerClient {
    pub fn new(base_url: &str, config: &PrivateZKPServerConfig) -> Self {
        PrivateZKPServerClient {
            client: build_client(),
            base_url: base_url.to_string(),
            config: config.clone(),
            pubkey: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn get_pubkey(&self) -> Result<RsaPublicKey, ServerError> {
        let is_pubkey_set = self.pubkey.read().unwrap().is_some();
        if !is_pubkey_set {
            let new_public_key = self.fetch_pubkey().await?;
            *self.pubkey.write().unwrap() = Some(new_public_key);
        }
        Ok(self.pubkey.read().unwrap().as_ref().unwrap().clone())
    }

    async fn fetch_pubkey(&self) -> Result<RsaPublicKey, ServerError> {
        let response: GetPublicKeyResponse =
            get_request::<(), _>(&self.client, &self.base_url, "/v1/public-key", None).await?;
        let public_key_bytes = BASE64_STANDARD.decode(&response.public_key).map_err(|e| {
            ServerError::DeserializationError(format!("Failed to decode public key: {e:?}"))
        })?;
        let public_key = RsaPublicKey::from_public_key_der(&public_key_bytes).map_err(|e| {
            ServerError::DeserializationError(format!("Failed to parse public key: {e:?}"))
        })?;
        Ok(public_key)
    }
}

#[async_trait(?Send)]
impl BalanceProverClientInterface for PrivateZKPServerClient {
    async fn prove_spent(
        &self,
        view_key: PrivateKey,
        spent_witness: &SpentWitness,
    ) -> Result<ProofWithPublicInputs<F, C, D>, ServerError> {
        let request = ProveSpentRequest {
            spent_witness: spent_witness.clone(),
        };
        let result = self
            .request_and_get_proof(
                view_key,
                &ProveRequestWithType {
                    prove_type: ProveType::Spent,
                    pubkey: view_key.to_public_key().0,
                    request: bincode::serialize(&request).unwrap(),
                },
            )
            .await?;
        self.handle_proof_result(result)
    }

    async fn prove_send(
        &self,
        view_key: PrivateKey,
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
        let result = self
            .request_and_get_proof(
                view_key,
                &ProveRequestWithType {
                    prove_type: ProveType::Send,
                    pubkey: view_key.to_public_key().0,
                    request: bincode::serialize(&request).unwrap(),
                },
            )
            .await?;
        self.handle_proof_result(result)
    }

    async fn prove_update(
        &self,
        view_key: PrivateKey,
        pubkey: U256,
        update_witness: &UpdateWitness<F, C, D>,
        prev_proof: &Option<ProofWithPublicInputs<F, C, D>>,
    ) -> Result<ProofWithPublicInputs<F, C, D>, ServerError> {
        let request = ProveUpdateRequest {
            pubkey,
            update_witness: update_witness.clone(),
            prev_proof: prev_proof.clone(),
        };
        let result = self
            .request_and_get_proof(
                view_key,
                &ProveRequestWithType {
                    prove_type: ProveType::Update,
                    pubkey: view_key.to_public_key().0,
                    request: bincode::serialize(&request).unwrap(),
                },
            )
            .await?;
        self.handle_proof_result(result)
    }

    async fn prove_receive_transfer(
        &self,
        view_key: PrivateKey,
        pubkey: U256,
        receive_transfer_witness: &ReceiveTransferWitness<F, C, D>,
        prev_proof: &Option<ProofWithPublicInputs<F, C, D>>,
    ) -> Result<ProofWithPublicInputs<F, C, D>, ServerError> {
        let request = ProveReceiveTransferRequest {
            pubkey,
            receive_transfer_witness: receive_transfer_witness.clone(),
            prev_proof: prev_proof.clone(),
        };
        let result = self
            .request_and_get_proof(
                view_key,
                &ProveRequestWithType {
                    prove_type: ProveType::ReceiveTransfer,
                    pubkey: view_key.to_public_key().0,
                    request: bincode::serialize(&request).unwrap(),
                },
            )
            .await?;
        self.handle_proof_result(result)
    }

    async fn prove_receive_deposit(
        &self,
        view_key: PrivateKey,
        pubkey: U256,
        receive_deposit_witness: &ReceiveDepositWitness,
        prev_proof: &Option<ProofWithPublicInputs<F, C, D>>,
    ) -> Result<ProofWithPublicInputs<F, C, D>, ServerError> {
        let request = ProveReceiveDepositRequest {
            pubkey,
            receive_deposit_witness: receive_deposit_witness.clone(),
            prev_proof: prev_proof.clone(),
        };
        let result = self
            .request_and_get_proof(
                view_key,
                &ProveRequestWithType {
                    prove_type: ProveType::ReceiveDeposit,
                    pubkey: view_key.to_public_key().0,
                    request: bincode::serialize(&request).unwrap(),
                },
            )
            .await?;
        self.handle_proof_result(result)
    }

    async fn prove_single_withdrawal(
        &self,
        view_key: PrivateKey,
        withdrawal_witness: &WithdrawalWitness<F, C, D>,
    ) -> Result<ProofWithPublicInputs<F, C, D>, ServerError> {
        let request = ProveSingleWithdrawalRequest {
            withdrawal_witness: withdrawal_witness.clone(),
        };
        let result = self
            .request_and_get_proof(
                view_key,
                &ProveRequestWithType {
                    prove_type: ProveType::SingleWithdrawal,
                    pubkey: view_key.to_public_key().0,
                    request: bincode::serialize(&request).unwrap(),
                },
            )
            .await?;
        self.handle_proof_result(result)
    }

    async fn prove_single_claim(
        &self,
        view_key: PrivateKey,
        is_faster_mining: bool,
        claim_witness: &ClaimWitness<F, C, D>,
    ) -> Result<ProofWithPublicInputs<F, C, D>, ServerError> {
        let request = ProveSingleClaimRequest {
            is_faster_mining,
            claim_witness: claim_witness.clone(),
        };
        let result = self
            .request_and_get_proof(
                view_key,
                &ProveRequestWithType {
                    prove_type: ProveType::SingleClaim,
                    pubkey: view_key.to_public_key().0,
                    request: bincode::serialize(&request).unwrap(),
                },
            )
            .await?;
        self.handle_proof_result(result)
    }
}

impl PrivateZKPServerClient {
    pub async fn send_prove_request(
        &self,
        request: &ProveRequestWithType,
    ) -> Result<String, ServerError> {
        let rsa_pubkey = self.get_pubkey().await?;
        let encrypted_request = request.encrypt_with_rsa(&rsa_pubkey);
        let encrypted_data = bincode::serialize(&encrypted_request).map_err(|e| {
            ServerError::SerializeError(format!("Failed to serialize encrypted request: {e:?}"))
        })?;
        let request = CreateProveRequest { encrypted_data };
        let response: CreateProofResponse = post_request(
            &self.client,
            &self.base_url,
            "/v1/proof/create",
            Some(&request),
        )
        .await?;
        Ok(response.request_id)
    }

    pub(crate) async fn get_request(
        &self,
        request_id: &str,
    ) -> Result<ProofResultResponse, ServerError> {
        let query = ProofResultQuery {
            request_id: request_id.to_string(),
        };
        let response: ProofResultResponse = get_request(
            &self.client,
            &self.base_url,
            "/v1/proof/result",
            Some(&query),
        )
        .await?;
        Ok(response)
    }

    pub async fn request_and_get_proof(
        &self,
        view_key: PrivateKey,
        request: &ProveRequestWithType,
    ) -> Result<ProofResultWithError, ServerError> {
        let request_id = self.send_prove_request(request).await?;
        let mut retries = 0;
        loop {
            let response = self.get_request(&request_id).await?;
            log::info!("{}: {}", request.prove_type, response.status);
            if response.status == "success" {
                if response.result.is_none() {
                    return Err(ServerError::InvalidResponse(format!(
                        "Proof result is missing: {}",
                        response.error.unwrap_or_default()
                    )));
                }

                let proof_with_result =
                    ProofResultWithError::decrypt(view_key, None, &response.result.unwrap())
                        .map_err(|e| {
                            ServerError::DeserializationError(format!(
                                "Failed to decrypt proof result: {e:?}"
                            ))
                        })?;

                return Ok(proof_with_result);
            } else if response.status == "error" {
                return Err(ServerError::InvalidResponse(format!(
                    "Proof request failed: {}",
                    response.error.unwrap_or_default()
                )));
            }

            if retries >= self.config.max_retries {
                return Err(ServerError::UnknownError(format!(
                    "Failed to get proof after {} retries",
                    self.config.max_retries
                )));
            }
            retries += 1;
            sleep_for(self.config.retry_interval).await;
        }
    }

    pub(crate) fn handle_proof_result(
        &self,
        proof_result: ProofResultWithError,
    ) -> Result<ProofWithPublicInputs<F, C, D>, ServerError> {
        if let Some(error) = proof_result.error {
            return Err(ServerError::InvalidResponse(format!(
                "Proof result contains error: {error}"
            )));
        }
        if proof_result.proof.is_none() {
            return Err(ServerError::InvalidResponse(
                "Proof result is missing proof".to_string(),
            ));
        }
        Ok(proof_result.proof.unwrap())
    }
}
