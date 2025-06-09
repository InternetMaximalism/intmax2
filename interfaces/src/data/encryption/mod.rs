use ::rsa::RsaPublicKey;
use bls::versioned_encryption::VersionedBlsEncryption;
use errors::{BlsEncryptionError, RsaEncryptionError};
use rsa::{decrypt_with_aes_key, encrypt_with_rsa, RsaEncryptedMessage};
use serde::{de::DeserializeOwned, Serialize};

use crate::utils::key::{PrivateKey, PublicKey};

pub mod bls;
pub mod errors;
pub mod rsa;

const LATEST_VERSION: u8 = 2;

pub trait BlsEncryption: Sized + Serialize + DeserializeOwned {
    fn from_bytes(bytes: &[u8], version: u8) -> Result<Self, BlsEncryptionError>;

    fn to_bytes(&self) -> Vec<u8> {
        bincode::serialize(self).unwrap()
    }

    fn encrypt(
        &self,
        receiver_view_pub: PublicKey,
        sender_key: Option<PrivateKey>, // Optional sender authentication key
    ) -> Result<Vec<u8>, BlsEncryptionError> {
        let data = self.to_bytes();
        let encrypted_data =
            VersionedBlsEncryption::encrypt(LATEST_VERSION, receiver_view_pub, sender_key, &data)?;
        Ok(bincode::serialize(&encrypted_data)?)
    }

    fn decrypt(
        receiver_view_priv: PrivateKey,
        sender_view_pub: Option<PublicKey>,
        encrypted_data: &[u8],
    ) -> Result<Self, BlsEncryptionError> {
        let data: VersionedBlsEncryption = bincode::deserialize(encrypted_data)?;
        let decrypted_data = data.decrypt(receiver_view_priv, sender_view_pub)?;
        let data = Self::from_bytes(&decrypted_data, data.version)?;
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
