use aes::{
    cipher::{KeyIvInit, StreamCipher},
    Aes128,
};
use alloy_primitives::B128;
use ark_std::Zero;
use ctr::Ctr64BE;
use intmax2_zkp::{
    common::signature::key_set::KeySet,
    ethereum_types::{u256::U256, u32limb_trait::U32LimbTrait},
};
use rand::Rng;

use crate::data::encryption::errors::ECIESError;

use super::{
    message::EncryptedMessage,
    utils::{ecdh_x, hmac_sha256, kdf, sha256, U256_SIZE},
};

pub use alloy_primitives::bytes::BytesMut;

const ENCRYPTION_VERSION: u8 = 1;

pub fn encrypt_bls(pubkey: U256, data: &[u8]) -> Vec<u8> {
    let sender = EciesSender::new(pubkey);
    let mut encrypted_data = BytesMut::new();
    sender.encrypt_message(data, &mut encrypted_data);

    let version = ENCRYPTION_VERSION;
    let mut version_data = BytesMut::new();
    version_data.extend_from_slice(&version.to_be_bytes());
    version_data.unsplit(encrypted_data);

    version_data.to_vec()
}

pub fn decrypt_bls(key: KeySet, encrypted_data: &[u8]) -> anyhow::Result<Vec<u8>> {
    let (version_data, encrypted_data) = encrypted_data.split_at(1);
    let version = version_data[0];
    if version != 1 {
        anyhow::bail!("Unsupported version");
    }

    if key.privkey.is_zero() {
        anyhow::bail!("Invalid private key");
    }

    let mut encrypted_data = encrypted_data.to_vec();
    let receiver = EciesReceiver::new(key);
    let decrypted_data = receiver.decrypt_message(&mut encrypted_data)?;

    Ok(decrypted_data.to_vec())
}

#[derive(Debug)]
pub struct EciesSender {
    pub receiver_public_key: U256,
}

impl EciesSender {
    pub fn new(receiver_public_key: U256) -> Self {
        Self {
            receiver_public_key,
        }
    }

    pub fn encrypt_message(&self, data: &[u8], out: &mut BytesMut) {
        let mut rng = rand::thread_rng();

        out.reserve(U256_SIZE + 16 + data.len() + 32);

        let total_size = U256_SIZE + 16 + data.len() + 32;
        let auth_tag: u16 = u16::try_from(total_size % 65536).unwrap(); // TODO: Is it correct?
        out.extend_from_slice(&auth_tag.to_be_bytes());

        let key = KeySet::rand(&mut rng);
        out.extend_from_slice(&key.pubkey.to_bytes_be()); // 32 bytes

        let receiver_public_key = self.receiver_public_key;
        let x = ecdh_x(&receiver_public_key, &key.privkey);
        let mut key = [0u8; 32];
        kdf(x, &[], &mut key);

        let enc_key = B128::from_slice(&key[..16]);
        let mac_key = sha256(&key[16..32]);

        let iv: B128 = rng.gen();
        let mut encryptor = Ctr64BE::<Aes128>::new((&enc_key.0).into(), (&iv.0).into());

        let mut encrypted = data.to_vec();
        encryptor.apply_keystream(&mut encrypted);

        let tag = hmac_sha256(
            mac_key.as_ref(),
            &[iv.as_slice(), &encrypted],
            &auth_tag.to_be_bytes(),
        );

        out.extend_from_slice(iv.as_slice());
        out.extend_from_slice(&encrypted);
        out.extend_from_slice(tag.as_ref());
    }
}

#[derive(Debug)]
pub struct EciesReceiver {
    pub key: KeySet,
}

impl EciesReceiver {
    pub fn new(key: KeySet) -> Self {
        Self { key }
    }

    pub fn decrypt_message<'a>(&self, data: &'a mut [u8]) -> Result<&'a mut [u8], ECIESError> {
        // parse the encrypted message from bytes
        let encrypted_message = EncryptedMessage::parse(data)?;

        // derive keys from the secret key and the encrypted message
        let keys = encrypted_message.derive_keys(&self.key.privkey);

        // check message integrity and decrypt the message
        encrypted_message.check_and_decrypt(keys)
    }
}

#[cfg(test)]
mod test {
    use aes::{
        cipher::{KeyIvInit, StreamCipher},
        Aes128,
    };
    use alloy_primitives::B128;
    use ctr::Ctr64BE;
    use intmax2_zkp::{
        common::signature::key_set::KeySet,
        ethereum_types::{u256::U256, u32limb_trait::U32LimbTrait},
    };
    use rand::Rng;

    use super::{
        decrypt_bls, ecdh_x, encrypt_bls, hmac_sha256, kdf, sha256, BytesMut, EciesReceiver,
        EciesSender, U256_SIZE,
    };

    #[test]
    fn test_ecies_encryption() {
        let mut rand = rand::thread_rng();
        let receiver_key = KeySet::rand(&mut rand);
        let sender = EciesSender::new(receiver_key.pubkey);

        let data = b"hello world";

        let mut encrypted_data = BytesMut::new();
        sender.encrypt_message(data, &mut encrypted_data);

        let receiver = EciesReceiver::new(receiver_key);
        let decrypted_data = receiver.decrypt_message(&mut encrypted_data).unwrap();

        assert_eq!(data.to_vec(), decrypted_data);
    }

    pub fn encrypt_message_with_invalid_auth_tag(
        receiver_public_key: U256,
        data: &[u8],
        out: &mut BytesMut,
    ) {
        let invalid_auth_tag = 0u16;

        let mut rng = rand::thread_rng();

        out.reserve(U256_SIZE + 16 + data.len() + 32);

        let total_size: u16 = u16::try_from(U256_SIZE + 16 + data.len() + 32).unwrap();

        out.extend_from_slice(&invalid_auth_tag.to_be_bytes());

        let key = KeySet::rand(&mut rng);
        out.extend_from_slice(&key.pubkey.to_bytes_be()); // 32 bytes

        let x = ecdh_x(&receiver_public_key, &key.privkey);
        let mut key = [0u8; 32];
        kdf(x, &[], &mut key);

        let enc_key = B128::from_slice(&key[..16]);
        let mac_key = sha256(&key[16..32]);

        let iv: B128 = rng.gen();
        let mut encryptor = Ctr64BE::<Aes128>::new((&enc_key.0).into(), (&iv.0).into());

        let mut encrypted = data.to_vec();
        encryptor.apply_keystream(&mut encrypted);

        let tag = hmac_sha256(
            mac_key.as_ref(),
            &[iv.as_slice(), &encrypted],
            &total_size.to_be_bytes(),
        );

        out.extend_from_slice(iv.as_slice());
        out.extend_from_slice(&encrypted);
        out.extend_from_slice(tag.as_ref());
    }

    #[test]
    fn test_ecies_encryption_with_invalid_auth_tag() {
        let mut rand = rand::thread_rng();
        let receiver_key = KeySet::rand(&mut rand);

        let data = b"hello world";

        let mut encrypted_data = BytesMut::new();
        encrypt_message_with_invalid_auth_tag(receiver_key.pubkey, data, &mut encrypted_data);

        let receiver = EciesReceiver::new(receiver_key);
        let decrypted_data = receiver.decrypt_message(&mut encrypted_data);

        match decrypted_data {
            Ok(_) => panic!("Decryption should fail"),
            Err(e) => {
                assert_eq!(e.to_string(), "tag check failure in read_header");
            }
        }
    }

    #[test]
    fn test_e2e_encryption() {
        let mut rand = rand::thread_rng();
        let key = KeySet::rand(&mut rand);

        let data = b"hello world";
        println!("data: {:?}", data.to_vec());

        let encrypted_data = encrypt_bls(key.pubkey, data);
        println!("encrypted data: {:?}", encrypted_data);

        let decrypted_data = decrypt_bls(key, &encrypted_data).unwrap();
        println!("decrypted data: {:?}", decrypted_data);

        assert_eq!(data.to_vec(), decrypted_data);
    }

    fn encrypt_with_unsupported_version(pubkey: U256, data: &[u8]) -> Vec<u8> {
        let sender = EciesSender::new(pubkey);
        let mut encrypted_data = BytesMut::new();
        sender.encrypt_message(data, &mut encrypted_data);

        let version = 2u8;
        let mut version_data = BytesMut::new();
        version_data.extend_from_slice(&version.to_be_bytes());
        version_data.unsplit(encrypted_data);

        version_data.to_vec()
    }

    #[test]
    fn test_e2e_encryption_with_unsupported_version() {
        let mut rand = rand::thread_rng();
        let key = KeySet::rand(&mut rand);

        let data = b"hello world";

        let encrypted_data = encrypt_with_unsupported_version(key.pubkey, data);

        let decrypted_data = decrypt_bls(key, &encrypted_data);

        match decrypted_data {
            Ok(_) => panic!("Decryption should fail"),
            Err(e) => {
                assert_eq!(e.to_string(), "Unsupported version");
            }
        }
    }
}
