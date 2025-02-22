// used for aws kms

use aes_gcm::{aead::Aead, AeadCore, Aes256Gcm, Key, KeyInit as _};
use rand::rngs::OsRng;
use rsa::{Oaep, RsaPrivateKey, RsaPublicKey};
use serde::{Deserialize, Serialize};
use serde_with::{base64::Base64, serde_as};
use sha2::Sha256;

use super::errors::RsaEncryptionError;

#[serde_as]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]

pub struct RsaEncryptedMessage {
    #[serde_as(as = "Base64")]
    pub nonce: [u8; 12],
    #[serde_as(as = "Base64")]
    pub ciphertext: Vec<u8>, // encrypted data via AES
    #[serde_as(as = "Base64")]
    pub encrypted_key: Vec<u8>, // encrypted aes key via RSA
}

pub fn encrypt_with_rsa(public_key: &RsaPublicKey, data: &[u8]) -> RsaEncryptedMessage {
    let aes_key = Aes256Gcm::generate_key(&mut OsRng);
    let aes_nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let cipher = Aes256Gcm::new(&aes_key);
    let ciphertext = cipher.encrypt(&aes_nonce, data.as_ref()).unwrap();
    let padding = Oaep::new::<Sha256>();
    let encrypted_key = public_key
        .encrypt(&mut OsRng, padding, aes_key.as_ref())
        .unwrap();

    RsaEncryptedMessage {
        nonce: aes_nonce.to_vec().try_into().unwrap(),
        ciphertext,
        encrypted_key,
    }
}

pub fn decrypt_aes_key(
    private_key: &RsaPrivateKey,
    encrypted_key: &[u8],
) -> Result<Vec<u8>, RsaEncryptionError> {
    let padding = Oaep::new::<Sha256>();
    let key = private_key.decrypt(padding, encrypted_key)?;
    Ok(key)
}

pub fn decrypt_with_aes_key(
    key: &[u8],
    message: &RsaEncryptedMessage,
) -> Result<Vec<u8>, RsaEncryptionError> {
    let aes_key = Key::<Aes256Gcm>::from_slice(key);
    let cipher = Aes256Gcm::new(aes_key);
    let nonce = message.nonce.into();
    let data = cipher
        .decrypt(&nonce, message.ciphertext.as_ref())
        .map_err(|e| {
            RsaEncryptionError::DecryptionError(format!("Error decrypting data: {}", e))
        })?;
    Ok(data.to_vec())
}

#[cfg(test)]
mod test {
    use rand::rngs::OsRng;
    use rsa::RsaPrivateKey;

    #[test]
    fn test_rsa_encrypt_decrypt() {
        let private_key = RsaPrivateKey::new(&mut OsRng, 3072).unwrap();
        let public_key = private_key.to_public_key();

        let data = b"hello world";
        let encrypted = super::encrypt_with_rsa(&public_key, data);

        let key = super::decrypt_aes_key(&private_key, &encrypted.encrypted_key).unwrap();
        let decrypted = super::decrypt_with_aes_key(&key, &encrypted).unwrap();

        assert_eq!(data.to_vec(), decrypted);
    }
}
