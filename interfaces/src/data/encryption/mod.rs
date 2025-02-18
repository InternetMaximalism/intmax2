use bls::algorithm::{decrypt, encrypt};
use errors::EncryptionError;
use intmax2_zkp::{common::signature::key_set::KeySet, ethereum_types::u256::U256};
use serde::{de::DeserializeOwned, Serialize};

pub mod bls;
pub mod errors;

pub trait Encryption: Sized + Serialize + DeserializeOwned {
    fn to_bytes(&self) -> Vec<u8> {
        bincode::serialize(self).unwrap()
    }

    fn from_bytes(bytes: &[u8]) -> Result<Self, EncryptionError> {
        let data = bincode::deserialize(bytes)?;
        Ok(data)
    }

    fn encrypt(&self, pubkey: U256) -> Vec<u8> {
        encrypt(pubkey, &self.to_bytes())
    }

    fn decrypt(bytes: &[u8], key: KeySet) -> Result<Self, EncryptionError> {
        let data =
            decrypt(key, bytes).map_err(|e| EncryptionError::DecryptionError(e.to_string()))?;
        let data = Self::from_bytes(&data)?;
        Ok(data)
    }
}
