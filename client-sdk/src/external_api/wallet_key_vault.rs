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
    async fn derive_mnemonic(
        &self,
        eth_private_key: B256,
    ) -> Result<Mnemonic<English>, ServerError> {
        let challenge_message = self
            .get_challenge_message(get_address_from_private_key(eth_private_key))
            .await?;
        let hashed_signature = self.login(eth_private_key, &challenge_message).await?;
        self.get_mnemonic(eth_private_key, hashed_signature).await
    }
}

impl WalletKeyVaultClient {
    pub fn new(base_url: String) -> Self {
        Self { base_url }
    }

    async fn sign_message(
        &self,
        private_key: B256,
        message: &str,
    ) -> Result<[u8; 65], ServerError> {
        let signer = PrivateKeySigner::from_bytes(&private_key).unwrap();
        let signature = signer
            .sign_message(message.as_bytes())
            .await
            .map_err(|e| ServerError::SigningError(format!("Failed to sign message: {}", e)))?;
        Ok(signature.as_bytes())
    }

    async fn signed_network_message(&self, private_key: B256) -> Result<[u8; 65], ServerError> {
        let address = get_address_from_private_key(private_key);
        self.sign_message(private_key, &network_message(address))
            .await
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
        let security_seed = sha256(&self.signed_network_message(private_key).await?);

        let request = LoginRequest {
            address: get_address_from_private_key(private_key),
            security_seed: encode_hex_with_prefix(&security_seed),
            challenge_signature: encode_hex_with_prefix(&signed_challenge_message),
        };
        let response: LoginResponse =
            post_request(&self.base_url, "/wallet/login", Some(&request)).await?;
        let hashed_signature = response.hashed_signature.clone();
        if hashed_signature.len() != 32 {
            return Err(ServerError::InvalidResponse(
                "Invalid hashed signature length".to_string(),
            ));
        }
        Ok(hashed_signature.try_into().unwrap())
    }

    async fn get_mnemonic(
        &self,
        private_key: B256,
        hashed_signature: [u8; 32],
    ) -> Result<Mnemonic<English>, ServerError> {
        let signed_network_message = self.signed_network_message(private_key).await?;
        let entropy =
            sha256(&[signed_network_message.to_vec(), hashed_signature.to_vec()].concat());
        let entropy: Entropy = entropy.into();
        let mnemonic = Mnemonic::<English>::new_from_entropy(entropy);
        Ok(mnemonic)
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

pub fn mnemonic_to_keyset(
    mnemonic: &Mnemonic<English>,
    redeposit_index: u32,
    wallet_index: u32,
) -> KeySet {
    let derive_path = format!("m/44'/60'/{redeposit_index}'/0/{wallet_index}");
    let derived_priv_key = mnemonic.derive_key(derive_path.as_str(), None).unwrap();
    let key: &SigningKey = derived_priv_key.as_ref();
    let signing_key = PrivateKeySigner::from_signing_key(key.clone());
    generate_intmax_account_from_eth_key(signing_key.to_bytes())
}

#[cfg(test)]
mod tests {

    use alloy::primitives::B256;
    use intmax2_zkp::ethereum_types::u32limb_trait::U32LimbTrait as _;

    use crate::external_api::{
        contract::utils::get_address_from_private_key, wallet_key_vault::mnemonic_to_keyset,
    };

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
        let mnemonic = client
            .get_mnemonic(private_key, hashed_signature)
            .await
            .unwrap();
        let keyset = mnemonic_to_keyset(&mnemonic, 0, 0);
        // dev environment
        assert_eq!(
            keyset.privkey.to_hex(),
            "0x03d97b592378ca1f7877087494f08fea97eeaea0a5ae65b3ea52c563370cb550"
        );
    }
}
