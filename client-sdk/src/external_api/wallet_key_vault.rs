use super::utils::query::post_request;
use crate::{
    client::key_from_eth::generate_intmax_account_from_eth_key,
    external_api::contract::utils::get_address_from_private_key,
};
use alloy::{
    primitives::{Address, B256},
    signers::{
        k256::ecdsa::SigningKey,
        local::{
            coins_bip39::{English, Entropy, Mnemonic},
            PrivateKeySigner,
        },
        Signer,
    },
};
use async_trait::async_trait;
use intmax2_interfaces::api::{
    error::ServerError,
    wallet_key_vault::{
        interface::WalletKeyVaultClientInterface,
        types::{ChallengeRequest, ChallengeResponse, LoginRequest, LoginResponse},
    },
};
use intmax2_zkp::common::signature_content::key_set::KeySet;
use sha2::Digest;

#[derive(Debug, Clone)]
pub struct WalletKeyVaultClient {
    pub base_url: String,
}

#[async_trait(?Send)]
impl WalletKeyVaultClientInterface for WalletKeyVaultClient {
    async fn derive_key_from_eth(&self, eth_private_key: B256) -> Result<KeySet, ServerError> {
        let challenge_message = self
            .get_challenge_message(get_address_from_private_key(eth_private_key))
            .await?;
        let hashed_signature = self.login(eth_private_key, &challenge_message).await?;
        self.get_keyset(eth_private_key, hashed_signature).await
    }
}

impl WalletKeyVaultClient {
    pub fn new(base_url: String) -> Self {
        Self { base_url }
    }

    async fn sign_message(&self, private_key: B256, message: &str) -> Result<Vec<u8>, ServerError> {
        let signer = PrivateKeySigner::from_bytes(&private_key).unwrap();
        let signature = signer
            .sign_message(message.as_bytes())
            .await
            .map_err(|e| ServerError::SigningError(format!("Failed to sign message: {}", e)))?;
        Ok(signature.as_bytes().to_vec())
    }

    async fn security_seed(&self, private_key: B256) -> Result<[u8; 32], ServerError> {
        let address = get_address_from_private_key(private_key);
        let signed_network_message = self
            .sign_message(private_key, &network_message(address))
            .await?;
        let security_seed = sha256(&signed_network_message);
        Ok(security_seed)
    }

    async fn get_challenge_message(&self, address: Address) -> Result<String, ServerError> {
        let request = ChallengeRequest {
            address,
            request_type: "login".to_string(),
        };
        let response: ChallengeResponse =
            post_request(&self.base_url, "/challenge", Some(&request)).await?;
        Ok(response.message)
    }

    async fn login(
        &self,
        private_key: B256,
        challenge_message: &str,
    ) -> Result<[u8; 32], ServerError> {
        let signed_challenge_message = self.sign_message(private_key, challenge_message).await?;
        let security_seed = self.security_seed(private_key).await?;

        let request = LoginRequest {
            address: get_address_from_private_key(private_key),
            security_seed: encode_hex_with_prefix(&security_seed),
            challenge_signature: encode_hex_with_prefix(&signed_challenge_message),
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

    async fn get_keyset(
        &self,
        private_key: B256,
        hashed_signature: [u8; 32],
    ) -> Result<KeySet, ServerError> {
        let security_seed = self.security_seed(private_key).await?;
        let entropy = sha256(&[security_seed, hashed_signature].concat());
        let entropy: Entropy = entropy.into();
        let mnemonic = Mnemonic::<English>::new_from_entropy(entropy);
        let signer = mnemonic_to_signer(&mnemonic)?;
        Ok(generate_intmax_account_from_eth_key(signer.to_bytes()))
    }
}

fn network_message(address: Address) -> String {
    format!(
        "\nThis signature on this message will be used to access the INTMAX network. \nYour address: {address}\nCaution: Please make sure that the domain you are connected to is correct."
    )
}

fn encode_hex_with_prefix(data: &[u8]) -> String {
    let hex = hex::encode(data);
    format!("0x{hex}")
}

fn sha256(data: &[u8]) -> [u8; 32] {
    let mut hasher = sha2::Sha256::new();
    hasher.update(data);
    hasher.finalize().into()
}

fn mnemonic_to_signer(mnemonic: &Mnemonic<English>) -> Result<PrivateKeySigner, ServerError> {
    let derived_priv_key = mnemonic.derive_key("m/44'/60'/0'/0/0", None).unwrap();
    let key: &SigningKey = derived_priv_key.as_ref();
    let signing_key = PrivateKeySigner::from_signing_key(key.clone());
    Ok(signing_key)
}

#[cfg(test)]
mod tests {
    use alloy::{primitives::B256, signers::local::MnemonicBuilder};
    use coins_bip39::{English, Mnemonic};
    use intmax2_zkp::ethereum_types::u32limb_trait::U32LimbTrait;

    use crate::external_api::contract::utils::get_address_from_private_key;

    use super::mnemonic_to_signer;

    fn get_client() -> super::WalletKeyVaultClient {
        let base_url = std::env::var("WALLET_KEY_VAULT_BASE_URL")
            .unwrap_or_else(|_| "http://localhost:3000".to_string());
        super::WalletKeyVaultClient::new(base_url)
    }

    #[tokio::test]
    #[ignore]
    async fn test_get_key() {
        let client = get_client();
        let private_key: B256 =
            "0x7397927abf5b7665c4667e8cb8b92e929e287625f79264564bb66c1fa2232b2c"
                .parse()
                .unwrap();
        let address = get_address_from_private_key(private_key);
        let challenge_message = client.get_challenge_message(address).await.unwrap();
        let hashed_signature = client.login(private_key, &challenge_message).await.unwrap();
        let keyset = client
            .get_keyset(private_key, hashed_signature)
            .await
            .unwrap();
        // dev environment
        assert_eq!(
            keyset.privkey.to_hex(),
            "0x293a2f74cbb6abde09244bb88b1e32c98799b01cf55d251ecc50338bd3b5b343"
        );
    }

    #[test]
    fn test_mnemonic_to_private_key() {
        let mnemonic_phrase = "bar retreat common buffalo van night stage artefact ring evil finger pelican best trade update sugar pave fossil weird camp coconut army swear aerobic";
        let mnemonic = Mnemonic::<English>::new_from_phrase(mnemonic_phrase).unwrap();
        let private_key = mnemonic_to_signer(&mnemonic).unwrap().to_bytes();

        let wallet = MnemonicBuilder::<English>::default()
            .phrase(mnemonic_phrase)
            .index(0)
            .unwrap()
            .build()
            .unwrap();

        assert_eq!(private_key, wallet.to_bytes());
    }
}
