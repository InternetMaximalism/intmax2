use ::rsa::RsaPublicKey;
use bls::versioned_encryption::VersionedBlsEncryption;
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

    fn encrypt(
        &self,
        receiver: U256,
        sender_key: Option<KeySet>,
    ) -> Result<Vec<u8>, BlsEncryptionError> {
        let data = self.to_bytes();
        let encrypted_data = VersionedBlsEncryption::encrypt(1, receiver, sender_key, &data)?;
        Ok(bincode::serialize(&encrypted_data)?)
    }

    fn decrypt(
        receiver_key: KeySet,
        sender: Option<U256>,
        encrypted_data: &[u8],
    ) -> Result<Self, BlsEncryptionError> {
        let data: VersionedBlsEncryption = bincode::deserialize(encrypted_data)?;
        let decrypted_data = data.decrypt(receiver_key, sender)?;
        let data = Self::from_bytes(&decrypted_data)?;
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
