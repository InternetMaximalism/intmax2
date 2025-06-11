use intmax2_zkp::{
    common::signature_content::{
        flatten::FlatG2,
        sign_tools::{sign_message, verify_signature},
    },
    ethereum_types::u256::U256,
};
use serde::{Deserialize, Serialize};
use sha2::Digest;

use crate::utils::key::PrivateKey;

#[derive(Default, Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Auth {
    pub pubkey: U256,
    pub expiry: u64,
    pub signature: FlatG2,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WithAuth<T> {
    pub inner: T,
    pub auth: Auth,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignContent {
    pub pubkey: U256,
    pub content: Vec<u8>,
    pub expiry: u64,
}

impl Auth {
    pub fn sign(private_key: PrivateKey, time_to_expiry: u64, content: &[u8]) -> Self {
        let expiry = current_time() + time_to_expiry;
        let pubkey = private_key.to_public_key();
        let sign_content = SignContent {
            pubkey: pubkey.0,
            content: content.to_vec(),
            expiry,
        };
        let serialized = bincode::serialize(&sign_content).unwrap();
        let digest = sha2::Sha256::digest(&serialized);
        let signature = sign_message(private_key.0, &digest).into();
        Auth {
            pubkey: pubkey.0,
            expiry,
            signature,
        }
    }

    pub fn verify(&self, content: &[u8]) -> anyhow::Result<()> {
        if self.expiry < current_time() {
            anyhow::bail!("Signature expired");
        }
        let sign_content = SignContent {
            pubkey: self.pubkey,
            content: content.to_vec(),
            expiry: self.expiry,
        };
        let serialized = bincode::serialize(&sign_content).unwrap();
        let digest = sha2::Sha256::digest(&serialized);
        verify_signature(self.signature.clone().into(), self.pubkey, &digest)?;
        Ok(())
    }
}

pub trait Signable: Sized {
    fn content(&self) -> Vec<u8>;

    fn sign(self, private_key: PrivateKey, time_to_expiry: u64) -> WithAuth<Self> {
        let auth = Auth::sign(private_key, time_to_expiry, &self.content());
        WithAuth { inner: self, auth }
    }

    fn verify(&self, auth: &Auth) -> anyhow::Result<()> {
        auth.verify(&self.content())
    }
}

pub fn current_time() -> u64 {
    chrono::Utc::now().timestamp() as u64
}

#[cfg(test)]
mod test {
    use crate::utils::{key::PrivateKey, random::default_rng};

    use super::{sign_message, verify_signature};
    use intmax2_zkp::{
        common::signature_content::key_set::KeySet,
        ethereum_types::{bytes32::Bytes32, u32limb_trait::U32LimbTrait},
    };

    #[test]
    fn test_sign_verify() {
        let mut rnd = default_rng();
        let key = KeySet::rand(&mut rnd);
        let hash = Bytes32::rand(&mut rnd);
        let signature = sign_message(key.privkey, &hash.to_bytes_be());
        assert!(verify_signature(signature, key.pubkey, &hash.to_bytes_be()).is_ok());
    }

    #[test]
    fn test_auth_verify() {
        let mut rnd = default_rng();
        let key = PrivateKey::rand(&mut rnd);
        let content = b"test";
        let auth = super::Auth::sign(key, 10, content);
        assert!(auth.verify(content).is_ok());
    }
}
