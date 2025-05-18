use std::io::Read;

use alloy::{
    primitives::{Address, B256},
    signers::{local::PrivateKeySigner, Signer},
};
use intmax2_interfaces::api::error::ServerError;
use intmax2_zkp::{common::signature_content::key_set::KeySet, ethereum_types::u256::U256};
use serde::{Deserialize, Serialize};
use sha2::Digest;

use crate::external_api::contract::utils::get_address_from_private_key;

use super::utils::query::post_request;

fn network_message(address: Address) -> String {
    format!(
        "\nThis signature on this message will be used to access the INTMAX network. \nYour address: {}\nCaution: Do not sign if requested on any domain other than intmax.io",
        address
    )
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChallengeRequest {
    pub address: String,
    #[serde(rename = "type")]
    pub request_type: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChallengeResponse {
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginRequest {
    pub address: Address,
    pub challenge_signature: String,
    pub security_seed: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginResponse {
    pub hashed_signature: String,
    pub nonce: u32,
    pub encrypted_entropy: Option<String>,
    pub access_token: Option<String>,
}

#[derive(Debug, Clone)]
pub struct WalletKeyVaultClient {
    pub base_url: String,
}

impl WalletKeyVaultClient {
    pub fn new(base_url: String) -> Self {
        Self { base_url }
    }

    async fn get_challenge(&self, address: Address) -> Result<String, ServerError> {
        let request = ChallengeRequest {
            address: address.to_string(),
            request_type: "login".to_string(),
        };
        let response: ChallengeResponse =
            post_request(&self.base_url, "/challenge", Some(&request)).await?;
        Ok(response.message)
    }

    async fn sign_message(&self, private_key: B256, message: &str) -> Result<Vec<u8>, ServerError> {
        let signer = PrivateKeySigner::from_bytes(&private_key).unwrap();
        let signature = signer
            .sign_message(message.as_bytes())
            .await
            .map_err(|e| ServerError::SigningError(format!("Failed to sign message: {}", e)))?;
        Ok(signature.as_bytes().to_vec())
    }

    async fn login(
        &self,
        private_key: B256,
        challenge_message: &str,
    ) -> Result<LoginResponse, ServerError> {
        let address = get_address_from_private_key(private_key);

        let signed_challenge_message = self.sign_message(private_key, challenge_message).await?;

        // hash the signed network message to create a security seed
        let signed_network_message = self
            .sign_message(private_key, &network_message(address))
            .await?;
        let security_seed = sha256(&signed_network_message);

        let request = LoginRequest {
            address,
            security_seed: "0x".to_string() + &hex::encode(security_seed),
            challenge_signature: "0x".to_string() + &hex::encode(signed_challenge_message),
        };
        let response: LoginResponse =
            post_request(&self.base_url, "/wallet/login", Some(&request)).await?;
        Ok(response)
    }

    async fn get_keyset(
        &self,
        private_key: B256,
        hashed_signature: &str,
    ) -> Result<KeySet, ServerError> {
        let address = get_address_from_private_key(private_key);
        let signed_network_message = self
            .sign_message(private_key, &network_message(address))
            .await?;
        let security_seed = sha256(&signed_network_message);

        let entropy_pre_image = hex::encode(security_seed) + hashed_signature;

        todo!()
    }
}

fn sha256(data: &[u8]) -> [u8; 32] {
    let mut hasher = sha2::Sha256::new();
    hasher.update(data);
    hasher.finalize().try_into().unwrap()
}

#[cfg(test)]
mod tests {
    use alloy::primitives::B256;

    use crate::external_api::contract::utils::get_address_from_private_key;

    fn get_client() -> super::WalletKeyVaultClient {
        let base_url = std::env::var("KEY_VAULT_BASE_URL")
            .unwrap_or_else(|_| "http://localhost:3000".to_string());
        super::WalletKeyVaultClient::new(base_url)
    }

    #[tokio::test]
    async fn test_get_challenge() {
        let client = get_client();
        let address = "0x1234567890abcdef1234567890abcdef12345678"
            .parse()
            .unwrap();
        let result = client.get_challenge(address).await.unwrap();
        dbg!(result);
    }

    #[tokio::test]
    async fn test_login() {
        let client = get_client();
        let private_key = B256::random();

        let address = get_address_from_private_key(private_key);
        let challenge_message = client.get_challenge(address).await.unwrap();

        let response = client.login(private_key, &challenge_message).await.unwrap();
        dbg!(response);
    }
}
