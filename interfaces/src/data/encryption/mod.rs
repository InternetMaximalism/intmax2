use ::rsa::RsaPublicKey;
use bls::algorithm::{decrypt_bls, encrypt_bls};
use errors::{BlsEncryptionError, RsaEncryptionError};
use intmax2_zkp::{common::signature::key_set::KeySet, ethereum_types::u256::U256};
use rsa::{decrypt_with_aes_key, encrypt_with_rsa, RsaEncryptedMessage};
use serde::{de::DeserializeOwned, Serialize};

pub mod bls;
pub mod errors;
pub mod rsa;

pub trait BlsEncryption: Sized + Serialize + DeserializeOwned {
    fn to_bytes(&self) -> Vec<u8> {
        bincode::serialize(self).unwrap()
    }

    fn from_bytes(bytes: &[u8]) -> Result<Self, BlsEncryptionError> {
        let data = bincode::deserialize(bytes)?;
        Ok(data)
    }

    fn encrypt(&self, pubkey: U256) -> Vec<u8> {
        encrypt_bls(pubkey, &self.to_bytes())
    }

    fn decrypt(bytes: &[u8], key: KeySet) -> Result<Self, BlsEncryptionError> {
        let data = decrypt_bls(key, bytes)
            .map_err(|e| BlsEncryptionError::DecryptionError(e.to_string()))?;
        let data = Self::from_bytes(&data)?;
        Ok(data)
    }
}

pub trait RsaEncryption: Sized + Serialize + DeserializeOwned {
    fn to_bytes(&self) -> Vec<u8> {
        bincode::serialize(self).unwrap()
    }

    fn from_bytes(bytes: &[u8]) -> Result<Self, RsaEncryptionError> {
        let data = bincode::deserialize(bytes)?;
        Ok(data)
    }

    fn encrypt_with_rsa(&self, pubkey: &RsaPublicKey) -> RsaEncryptedMessage {
        encrypt_with_rsa(pubkey, &self.to_bytes())
    }

    fn decrypt_with_aes_key(
        key: &[u8],
        encrypted: &RsaEncryptedMessage,
    ) -> Result<Self, RsaEncryptionError> {
        let data = decrypt_with_aes_key(key, encrypted)?;
        let data = Self::from_bytes(&data)?;
        Ok(data)
    }
}
