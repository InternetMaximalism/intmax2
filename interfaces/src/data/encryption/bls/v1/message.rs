//! These functions reference reth.
//! <https://github.com/paradigmxyz/reth/blob/main/crates/net/ecies/src/algorithm.rs>

use aes::{
    cipher::{KeyIvInit, StreamCipher},
    Aes128,
};
use alloy_primitives::{B128, B256};
use ark_bn254::Fr;
use ctr::Ctr64BE;
use intmax2_zkp::ethereum_types::{u256::U256, u32limb_trait::U32LimbTrait};

use super::{
    error::ECIESError,
    utils::{ecdh_x, hmac_sha256, kdf, sha256, U256_SIZE},
};

/// The symmetric keys derived from an ECIES message.
#[derive(Debug)]
pub struct RLPxSymmetricKeys {
    /// The key used for decryption, specifically with AES-128 in CTR mode, using a 64-bit big
    /// endian counter.
    pub enc_key: B128,

    /// The key used for verifying message integrity, specifically with the NIST SP 800-56A Concat
    /// KDF.
    pub mac_key: B256,
}

#[derive(Debug)]
pub struct EncryptedMessage<'a> {
    /// The auth data, used when checking the `tag` with HMAC-SHA256.
    ///
    /// This is not mentioned in the `RLPx` spec, but included in implementations.
    ///
    /// See source comments of [`Self::check_integrity`] for more information.
    auth_data: [u8; 2],
    /// The remote secp256k1 public key
    public_key: U256,
    /// The IV, for use in AES during decryption, in the tag check
    iv: B128,
    /// The encrypted data
    encrypted_data: &'a mut [u8],
    /// The message tag
    tag: B256,
}

impl<'a> EncryptedMessage<'a> {
    /// Parse the given `data` into an [`EncryptedMessage`].
    ///
    /// If the data is not long enough to contain the expected fields, this returns an error.
    pub fn parse(data: &mut [u8]) -> Result<EncryptedMessage<'_>, ECIESError> {
        // Auth data is 2 bytes, public key is 65 bytes
        if data.len() < U256_SIZE + 2 {
            return Err(ECIESError::EncryptedDataTooSmall);
        }
        let (auth_data, encrypted) = data.split_at_mut(2);

        // convert the auth data to a fixed size array
        //
        // NOTE: this will not panic because we've already checked that the data is long enough
        let auth_data = auth_data.try_into().unwrap();

        let (pubkey_bytes, encrypted) = encrypted.split_at_mut(U256_SIZE);
        let public_key = U256::from_bytes_be(pubkey_bytes);

        // return an error if the encrypted len is currently less than 32
        let tag_index = encrypted
            .len()
            .checked_sub(32)
            .ok_or(ECIESError::EncryptedDataTooSmall)?;

        // NOTE: we've already checked that the encrypted data is long enough to contain the
        // encrypted data and tag
        let (data_iv, tag_bytes) = encrypted.split_at_mut(tag_index);

        // NOTE: this will not panic because we are splitting at length minus 32 bytes, which
        // causes tag_bytes to be 32 bytes long
        let tag = B256::from_slice(tag_bytes);

        // now we can check if the encrypted data is long enough to contain the IV
        if data_iv.len() < 16 {
            return Err(ECIESError::EncryptedDataTooSmall);
        }
        let (iv, encrypted_data) = data_iv.split_at_mut(16);

        // NOTE: this will not panic because we are splitting at 16 bytes
        let iv = B128::from_slice(iv);

        Ok(EncryptedMessage {
            auth_data,
            public_key,
            iv,
            encrypted_data,
            tag,
        })
    }

    /// Use the given secret and this encrypted message to derive the shared secret, and use the
    /// shared secret to derive the mac and encryption keys.
    pub fn derive_keys(&self, secret_key: &Fr) -> RLPxSymmetricKeys {
        // perform ECDH to get the shared secret, using the remote public key from the message and
        // the given secret key
        let x = ecdh_x(&self.public_key, secret_key);
        let mut key = [0u8; 32];

        // The RLPx spec describes the key derivation process as:
        //
        // kE || kM = KDF(S, 32)
        //
        // where kE is the encryption key, and kM is used to determine the MAC key (see below)
        //
        // NOTE: The RLPx spec does not define an `OtherInfo` parameter, and this is unused in
        // other implementations, so we use an empty slice.
        kdf(x, &[], &mut key);

        let enc_key = B128::from_slice(&key[..16]);

        // The MAC tag check operation described is:
        //
        // d == MAC(sha256(kM), iv || c)
        //
        // where kM is the result of the above KDF, iv is the IV, and c is the encrypted data.
        // Because the hash of kM is ultimately used as the mac key, we perform that hashing here.
        let mac_key = sha256(&key[16..32]);

        RLPxSymmetricKeys { enc_key, mac_key }
    }

    /// Use the given ECIES keys to check the message integrity using the contained tag.
    pub fn check_integrity(&self, keys: &RLPxSymmetricKeys) -> Result<(), ECIESError> {
        // The MAC tag check operation described is:
        //
        // d == MAC(sha256(kM), iv || c)
        //
        // NOTE: The RLPx spec does not show here that the `auth_data` is required for checking the
        // tag.
        //
        // Geth refers to SEC 1's definition of ECIES:
        //
        // Encrypt encrypts a message using ECIES as specified in SEC 1, section 5.1.
        //
        // s1 and s2 contain shared information that is not part of the resulting
        // ciphertext. s1 is fed into key derivation, s2 is fed into the MAC. If the
        // shared information parameters aren't being used, they should be nil.
        //
        // ```
        // prefix := make([]byte, 2)
        // binary.BigEndian.PutUint16(prefix, uint16(len(h.wbuf.data)+eciesOverhead))
        //
        // enc, err := ecies.Encrypt(rand.Reader, h.remote, h.wbuf.data, nil, prefix)
        // ```
        let check_tag = hmac_sha256(
            keys.mac_key.as_ref(),
            &[self.iv.as_slice(), self.encrypted_data],
            &self.auth_data,
        );
        if check_tag != self.tag {
            return Err(ECIESError::TagCheckDecryptFailed);
        }

        Ok(())
    }

    /// Use the given ECIES keys to decrypt the contained encrypted data, consuming the message and
    /// returning the decrypted data.
    pub fn decrypt(self, keys: &RLPxSymmetricKeys) -> &'a mut [u8] {
        let Self {
            iv, encrypted_data, ..
        } = self;

        // rename for clarity once it's decrypted
        let decrypted_data = encrypted_data;

        let mut decryptor = Ctr64BE::<Aes128>::new((&keys.enc_key.0).into(), (&*iv).into());
        decryptor.apply_keystream(decrypted_data);
        decrypted_data
    }

    /// Use the given ECIES keys to check the integrity of the message, returning an error if the
    /// tag check fails, and then decrypt the message, returning the decrypted data.
    pub fn check_and_decrypt(self, keys: RLPxSymmetricKeys) -> Result<&'a mut [u8], ECIESError> {
        self.check_integrity(&keys)?;
        Ok(self.decrypt(&keys))
    }
}
