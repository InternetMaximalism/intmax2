use alloy::{
    primitives::{Address, B256},
    signers::{
        local::{
            coins_bip39::{English, Entropy, Mnemonic},
            MnemonicBuilder, PrivateKeySigner,
        },
        Signer,
    },
};
use intmax2_interfaces::api::error::ServerError;
use intmax2_zkp::common::signature_content::key_set::KeySet;
use serde::{Deserialize, Serialize};
use serde_with::{base64::Base64, serde_as};
use sha2::Digest;

use crate::{
    client::key_from_eth::generate_intmax_account_from_eth_key,
    external_api::contract::utils::get_address_from_private_key,
};

use super::utils::query::post_request;

fn network_message(address: Address) -> String {
    format!(
        "\nThis signature on this message will be used to access the INTMAX network. \nYour address: {address}\nCaution: Please make sure that the domain you are connected to is correct."
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

#[serde_as]
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginResponse {
    #[serde_as(as = "Base64")]
    pub hashed_signature: Vec<u8>,
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

    pub async fn get_challenge(&self, address: Address) -> Result<String, ServerError> {
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

    async fn get_security_seed(&self, private_key: B256) -> Result<[u8; 32], ServerError> {
        let address = get_address_from_private_key(private_key);
        let signed_network_message = self
            .sign_message(private_key, &network_message(address))
            .await?;
        dbg!(&network_message(address));
        let security_seed = sha256(&signed_network_message);
        Ok(security_seed)
    }

    pub async fn login(
        &self,
        private_key: B256,
        challenge_message: &str,
    ) -> Result<[u8; 32], ServerError> {
        let address = get_address_from_private_key(private_key);

        let signed_challenge_message = self.sign_message(private_key, challenge_message).await?;
        let security_seed = self.get_security_seed(private_key).await?;

        let request = LoginRequest {
            address,
            security_seed: "0x".to_string() + &hex::encode(security_seed),
            challenge_signature: "0x".to_string() + &hex::encode(signed_challenge_message),
        };
        let response: LoginResponse =
            post_request(&self.base_url, "/wallet/login", Some(&request)).await?;
        let mut hashed_signature = response.hashed_signature.clone();
        if response.hashed_signature.len() > 32 {
            return Err(ServerError::InvalidResponse(
                "Invalid hashed signature length".to_string(),
            ));
        }
        hashed_signature.resize(32, 0);
        Ok(hashed_signature.try_into().unwrap())
    }

    pub async fn get_keyset(
        &self,
        private_key: B256,
        hashed_signature: [u8; 32],
    ) -> Result<KeySet, ServerError> {
        let security_seed = self.get_security_seed(private_key).await?;
        let entropy = sha256(&[security_seed, hashed_signature].concat());
        let entropy: Entropy = entropy.into();
        let mnemonic = Mnemonic::<English>::new_from_entropy(entropy);
        dbg!(mnemonic.to_phrase());
        let wallet = MnemonicBuilder::<English>::default()
            .phrase(mnemonic.to_phrase())
            .index(0)
            .unwrap()
            .build()
            .unwrap();
        let eth_private_key = wallet.to_bytes();
        Ok(generate_intmax_account_from_eth_key(eth_private_key))
    }
}

fn sha256(data: &[u8]) -> [u8; 32] {
    let mut hasher = sha2::Sha256::new();
    hasher.update(data);
    hasher.finalize().into()
}

#[cfg(test)]
mod tests {
    use alloy::primitives::B256;
    use intmax2_zkp::ethereum_types::u32limb_trait::U32LimbTrait;

    use crate::external_api::contract::utils::get_address_from_private_key;

    fn get_client() -> super::WalletKeyVaultClient {
        let base_url = std::env::var("WALLET_KEY_VAULT_BASE_URL")
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

    #[tokio::test]
    async fn test_get_key() {
        let client = get_client();
        let private_key: B256 =
            "0x7397927abf5b7665c4667e8cb8b92e929e287625f79264564bb66c1fa2232b2c"
                .parse()
                .unwrap();

        let address = get_address_from_private_key(private_key);
        let challenge_message = client.get_challenge(address).await.unwrap();
        let hashed_signature = client.login(private_key, &challenge_message).await.unwrap();
        let keyset = client
            .get_keyset(private_key, hashed_signature)
            .await
            .unwrap();

        assert_eq!(
            keyset.privkey.to_hex(),
            "0x2b9321ca673e7865bac8fafb81a1f23ff29693c2e9c3523bc0f6bbf7b4087bcd"
        );
    }
}
