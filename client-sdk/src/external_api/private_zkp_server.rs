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
};
use intmax2_zkp::{
    common::{
        signature::key_set::KeySet,
        witness::{
            claim_witness::ClaimWitness, receive_deposit_witness::ReceiveDepositWitness,
            receive_transfer_witness::ReceiveTransferWitness, spent_witness::SpentWitness,
            tx_witness::TxWitness, update_witness::UpdateWitness,
            withdrawal_witness::WithdrawalWitness,
        },
    },
    ethereum_types::{u256::U256, u32limb_trait::U32LimbTrait},
};
use plonky2::{
    field::goldilocks_field::GoldilocksField,
    plonk::{config::PoseidonGoldilocksConfig, proof::ProofWithPublicInputs},
};
use rsa::{pkcs8::DecodePublicKey, RsaPublicKey};
use serde::{Deserialize, Serialize};

use crate::external_api::utils::time::sleep_for;

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
    base_url: String,

    config: PrivateZKPServerConfig,

    // rsa public key is used to encrypt the prove request
    // because async OnceLock is not stable, we use RwLock + Option instead
    pubkey: Arc<RwLock<Option<RsaPublicKey>>>,
}

impl PrivateZKPServerClient {
    pub fn new(base_url: &str, config: &PrivateZKPServerConfig) -> Self {
        PrivateZKPServerClient {
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
            get_request::<(), _>(&self.base_url, "/v1/public-key", None).await?;

        // let public_key = "MIIBojANBgkqhkiG9w0BAQEFAAOCAY8AMIIBigKCAYEA1CH9K5XR1+f4rVEakUhLZbMh7t+LlL163IpFGlvzaulhPkZQNLk1XGnSPzxMlcTBb8FBmnrW18TlXfVxRjMl6pGEahi56sjLrFItGzqMyRn4JvLIfs7NnYBIqS86UtEtSHgpiQ/l94GJxOqYpZVTppcquORPhHSPp/CIWRWl4QYgbb4b6oUV71ltrKeD36XlCdbJlYsv8tVinbe/bdp+230FgOYXgZasCCWTNJH2QJ5UkfaPANYWY6YE3/BPi1af9Nb/AqRVbbWmI65C+K7RSEjO6/LdiNq+rbQF203xfYKOFUZwyoCbo0UD6gjZmbt15vVavmJSUcXxmYaPvkO+yT0f4iPTa1oSQwatAxd6vLTYYak20jY/o8N9K44V21im28m4m90w5Aq6ba2TplTXMf8oo3B/KYEbBEUfZam3RvliaHThzR80qzpd+kGXG9s8f3cK1YP/dkroWZrrH4UU5zLBj8XCYXBUEMek7ezkORpzEeWYzMhbsl5wbFSkPqujAgMBAAE=";
        let public_key = &response.public_key;
        let public_key_bytes = BASE64_STANDARD.decode(public_key).map_err(|e| {
            ServerError::DeserializationError(format!("Failed to decode public key: {:?}", e))
        })?;
        let public_key = RsaPublicKey::from_public_key_der(&public_key_bytes).map_err(|e| {
            ServerError::DeserializationError(format!("Failed to parse public key: {:?}", e))
        })?;
        Ok(public_key)
    }
}

#[async_trait(?Send)]
impl BalanceProverClientInterface for PrivateZKPServerClient {
    async fn prove_spent(
        &self,
        key: KeySet,
        spent_witness: &SpentWitness,
    ) -> Result<ProofWithPublicInputs<F, C, D>, ServerError> {
        let request = ProveSpentRequest {
            spent_witness: spent_witness.clone(),
        };
        log::info!("prove_spent: {}", key.pubkey.to_hex());
        let result = self
            .request_and_get_proof(
                key,
                &ProveRequestWithType {
                    prove_type: ProveType::Spent,
                    pubkey: key.pubkey,
                    request: bincode::serialize(&request).unwrap(),
                },
            )
            .await?;
        self.handle_proof_result(result)
    }

    async fn prove_send(
        &self,
        key: KeySet,
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
        log::info!("prove_send: {}", key.pubkey.to_hex());
        let result = self
            .request_and_get_proof(
                key,
                &ProveRequestWithType {
                    prove_type: ProveType::Send,
                    pubkey: key.pubkey,
                    request: bincode::serialize(&request).unwrap(),
                },
            )
            .await?;
        self.handle_proof_result(result)
    }

    async fn prove_update(
        &self,
        key: KeySet,
        pubkey: U256,
        update_witness: &UpdateWitness<F, C, D>,
        prev_proof: &Option<ProofWithPublicInputs<F, C, D>>,
    ) -> Result<ProofWithPublicInputs<F, C, D>, ServerError> {
        let request = ProveUpdateRequest {
            pubkey,
            update_witness: update_witness.clone(),
            prev_proof: prev_proof.clone(),
        };
        log::info!("prove_update: {}", key.pubkey.to_hex());
        let result = self
            .request_and_get_proof(
                key,
                &ProveRequestWithType {
                    prove_type: ProveType::Update,
                    pubkey: key.pubkey,
                    request: bincode::serialize(&request).unwrap(),
                },
            )
            .await?;
        self.handle_proof_result(result)
    }

    async fn prove_receive_transfer(
        &self,
        key: KeySet,
        pubkey: U256,
        receive_transfer_witness: &ReceiveTransferWitness<F, C, D>,
        prev_proof: &Option<ProofWithPublicInputs<F, C, D>>,
    ) -> Result<ProofWithPublicInputs<F, C, D>, ServerError> {
        let request = ProveReceiveTransferRequest {
            pubkey,
            receive_transfer_witness: receive_transfer_witness.clone(),
            prev_proof: prev_proof.clone(),
        };
        log::info!("prove_receive_transfer: {}", key.pubkey.to_hex());
        let result = self
            .request_and_get_proof(
                key,
                &ProveRequestWithType {
                    prove_type: ProveType::ReceiveTransfer,
                    pubkey: key.pubkey,
                    request: bincode::serialize(&request).unwrap(),
                },
            )
            .await?;
        self.handle_proof_result(result)
    }

    async fn prove_receive_deposit(
        &self,
        key: KeySet,
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
                key,
                &ProveRequestWithType {
                    prove_type: ProveType::ReceiveDeposit,
                    pubkey: key.pubkey,
                    request: bincode::serialize(&request).unwrap(),
                },
            )
            .await?;
        self.handle_proof_result(result)
    }

    async fn prove_single_withdrawal(
        &self,
        key: KeySet,
        withdrawal_witness: &WithdrawalWitness<F, C, D>,
    ) -> Result<ProofWithPublicInputs<F, C, D>, ServerError> {
        let request = ProveSingleWithdrawalRequest {
            withdrawal_witness: withdrawal_witness.clone(),
        };
        log::info!("prove_single_withdrawal: {}", key.pubkey.to_hex());
        let result = self
            .request_and_get_proof(
                key,
                &ProveRequestWithType {
                    prove_type: ProveType::SingleWithdrawal,
                    pubkey: key.pubkey,
                    request: bincode::serialize(&request).unwrap(),
                },
            )
            .await?;
        self.handle_proof_result(result)
    }

    async fn prove_single_claim(
        &self,
        _key: KeySet,
        claim_witness: &ClaimWitness<F, C, D>,
    ) -> Result<ProofWithPublicInputs<F, C, D>, ServerError> {
        let request = ProveSingleClaimRequest {
            claim_witness: claim_witness.clone(),
        };
        log::info!("prove_single_claim: {}", _key.pubkey.to_hex());
        let result = self
            .request_and_get_proof(
                _key,
                &ProveRequestWithType {
                    prove_type: ProveType::SingleClaim,
                    pubkey: _key.pubkey,
                    request: bincode::serialize(&request).unwrap(),
                },
            )
            .await?;
        self.handle_proof_result(result)
    }
}

impl PrivateZKPServerClient {
    pub(crate) async fn send_prove_request(
        &self,
        request: &ProveRequestWithType,
    ) -> Result<String, ServerError> {
        let rsa_pubkey = self.get_pubkey().await?;
        let encrypted_request = request.encrypt_with_rsa(&rsa_pubkey);
        let encrypted_data = bincode::serialize(&encrypted_request).map_err(|e| {
            ServerError::SerializeError(format!("Failed to serialize encrypted request: {:?}", e))
        })?;
        let request = CreateProveRequest { encrypted_data };
        let response: CreateProofResponse =
            post_request(&self.base_url, "/v1/proof/create", Some(&request)).await?;
        Ok(response.request_id)
    }

    pub(crate) async fn get_request(
        &self,
        request_id: &str,
    ) -> Result<ProofResultResponse, ServerError> {
        let query = ProofResultQuery {
            request_id: request_id.to_string(),
        };
        let response: ProofResultResponse =
            get_request(&self.base_url, "/v1/proof/result", Some(&query)).await?;
        Ok(response)
    }

    pub(crate) async fn request_and_get_proof(
        &self,
        key: KeySet,
        request: &ProveRequestWithType,
    ) -> Result<ProofResultWithError, ServerError> {
        let request_id = self.send_prove_request(request).await?;
        log::info!(
            "(request_and_get_proof) request_id={}, pubkey={}",
            key.pubkey.to_hex(),
            request_id
        );
        let mut retries = 0;
        let start = std::time::Instant::now();
        loop {
            let response = self.get_request(&request_id).await?;
            log::info!("prove_type {}: {}", request.prove_type, response.status);
            if response.status == "success" {
                if response.result.is_none() {
                    return Err(ServerError::InvalidResponse(format!(
                        "Proof result (request_id={}, pubkey={}) is missing: {}",
                        request_id,
                        key.pubkey.to_hex(),
                        response.error.unwrap_or_default()
                    )));
                }

                let proof_with_result =
                    ProofResultWithError::decrypt(key, None, &response.result.unwrap()).map_err(
                        |e| {
                            ServerError::DeserializationError(format!(
                                "Failed to decrypt proof result: {:?}",
                                e
                            ))
                        },
                    )?;
                log::info!(
                    "Success proof generation (request_id={}, pubkey={}) {} ({}s)",
                    request_id,
                    key.pubkey.to_hex(),
                    request.prove_type,
                    start.elapsed().as_secs()
                );

                return Ok(proof_with_result);
            } else if response.status == "error" {
                return Err(ServerError::InvalidResponse(format!(
                    "Proof request failed (request_id={}, pubkey={}): {}",
                    request_id,
                    key.pubkey.to_hex(),
                    response.error.unwrap_or_default()
                )));
            }

            if retries >= self.config.max_retries {
                return Err(ServerError::UnknownError(format!(
                    "Failed to get proof (request_id={}, pubkey={}) after {} retries ({}s)",
                    request_id,
                    key.pubkey.to_hex(),
                    self.config.max_retries,
                    start.elapsed().as_secs()
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
                "Proof result contains error: {}",
                error
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
