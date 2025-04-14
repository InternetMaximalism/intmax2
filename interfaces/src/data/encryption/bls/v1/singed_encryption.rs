use intmax2_zkp::{
    common::signature_content::{
        flatten::FlatG2,
        key_set::KeySet,
        sign_tools::{sign_message, verify_signature},
    },
    ethereum_types::{u256::U256, u32limb_trait::U32LimbTrait},
};
use serde::{Deserialize, Serialize};

use crate::{
    data::encryption::bls::v1::algorithm::{decrypt_bls, encrypt_bls},
    utils::digest::get_digest,
};

#[derive(Debug, thiserror::Error)]
pub enum V1SignedEncryptionError {
    #[error("signature not found")]
    SignatureNotFound,

    #[error("signature verification failed")]
    SignatureVerificationFailed(String),

    #[error("decryption error: {0}")]
    DecryptionError(String),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct V1SignedEncryption {
    pub data: Vec<u8>,
    pub signature: Option<FlatG2>,
}

impl V1SignedEncryption {
    pub fn encrypt(receiver: U256, sender_key: Option<KeySet>, data: &[u8]) -> Self {
        let data = encrypt_bls(receiver, data);
        let digest = get_digest(&data);
        let signature: Option<FlatG2> =
            sender_key.map(|key| sign_message(key.privkey, &digest.to_bytes_be()).into());
        Self { data, signature }
    }

    pub fn decrypt(
        &self,
        receiver_key: KeySet,
        sender: Option<U256>,
    ) -> Result<Vec<u8>, V1SignedEncryptionError> {
        // verify signature
        if let Some(sender) = sender {
            let signature = self
                .signature
                .clone()
                .ok_or(V1SignedEncryptionError::SignatureNotFound)?;
            let digest = get_digest(&self.data);
            verify_signature(signature.into(), sender, &digest.to_bytes_be())
                .map_err(|e| V1SignedEncryptionError::SignatureVerificationFailed(e.to_string()))?;
        }
        let data = decrypt_bls(receiver_key, &self.data)
            .map_err(|e| V1SignedEncryptionError::DecryptionError(e.to_string()))?;
        Ok(data)
    }
}
