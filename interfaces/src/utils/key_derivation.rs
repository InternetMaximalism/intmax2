use crate::utils::key::{KeyPair, PrivateKey};
use ark_bn254::Fr;
use hkdf::Hkdf;
use intmax2_zkp::{
    common::signature_content::key_set::KeySet,
    ethereum_types::{bytes32::Bytes32, u256::U256, u32limb_trait::U32LimbTrait},
};
use num_bigint::BigUint;
use num_traits::identities::Zero;
use sha2::{Digest, Sha512};

/// Derives a keypair (spend key and view key) from a spend key.
/// If `is_legacy` is true, it uses the spend key as the view key.
pub fn derive_keypair_from_spend_key(spend_key: PrivateKey, is_legacy: bool) -> KeyPair {
    let view_key = if is_legacy {
        spend_key
    } else {
        derive_view_key_from_spend_key(&spend_key)
    };
    KeyPair {
        spend: spend_key,
        view: view_key,
    }
}

/// Derives a spend key from a 32-byte input using SHA-512 hashing.
pub fn derive_spend_key_from_bytes32(input: Bytes32) -> PrivateKey {
    let mut hasher = Sha512::new();
    loop {
        hasher.update(b"INTMAX");
        hasher.update(input.to_bytes_be());
        let digest = hasher.clone().finalize();
        let provisional_private_key: Fr = BigUint::from_bytes_be(&digest).into();
        if provisional_private_key.is_zero() {
            continue;
        }
        let provisional_private_key: U256 =
            BigUint::from(provisional_private_key).try_into().unwrap();
        let key = KeySet::new(provisional_private_key);
        return PrivateKey(key.privkey);
    }
}

// Derives a view key from a spend key using HKDF with SHA-512.
pub fn derive_view_key_from_spend_key(spend_key: &PrivateKey) -> PrivateKey {
    let salt = [0u8; 32];
    let info = b"spend-key-to-view-key";
    let hk = Hkdf::<Sha512>::new(Some(&salt), &spend_key.to_bytes());
    let mut derived_key = [0u8; 64];
    hk.expand(info, &mut derived_key).unwrap();
    let derived_key: Fr = BigUint::from_bytes_be(&derived_key).into();
    let derived_key: U256 = BigUint::from(derived_key).try_into().unwrap();
    let key = KeySet::new(derived_key);
    PrivateKey(key.privkey)
}

#[cfg(test)]
mod test {
    use crate::utils::{key::PrivateKey, key_derivation::derive_view_key_from_spend_key};

    use super::derive_spend_key_from_bytes32;
    use intmax2_zkp::ethereum_types::{bytes32::Bytes32, u32limb_trait::U32LimbTrait};

    struct SpendKeyTestCase {
        input: Bytes32,
        public_key: String,
    }

    struct ViewKeyTestCase {
        input: PrivateKey,
        output: String,
    }

    #[test]
    fn test_derive_spend_key_from_bytes32() {
        let test_cases = [
            SpendKeyTestCase {
                input: "f68ff926147a67518161e65cd54a3a44c2379e4b63c74b52cfc74274d2586299"
                    .parse()
                    .unwrap(),
                public_key: "0x2f2ddf326b1b4528706ecab6ff465b15cc1f4a4a2d8ea5d39d66ffb0a91a277c"
                    .to_string(),
            },
            SpendKeyTestCase {
                input: "3db985c15e2788a9f03a797c71151571cbbd0cb2a89402f640102cb8b445e59a"
                    .parse()
                    .unwrap(),
                public_key: "0x17aebd78d4259e734ba1c9ce1b58c9adea5ab3e68c61e6251dd3016085101941"
                    .to_string(),
            },
            SpendKeyTestCase {
                input: "962bc2ea6e76fc3863906a894f3b17cce375ff298c7c5efcf0d4ce9d054e7e4e"
                    .parse()
                    .unwrap(),
                public_key: "0x1fb62949642c57749922484377541e70445881599cfb19c74066fe0f885510af"
                    .to_string(),
            },
            SpendKeyTestCase {
                input: "25be37b3ca8370a172765133f23c849905f21ed2dd90422bc8901cbbe69e3e1c"
                    .parse()
                    .unwrap(),
                public_key: "0x2c8ffeb9b3a365c0387f841973defbb203be92a509f075a0821aaeec79f7080f"
                    .to_string(),
            },
        ];

        for test_case in test_cases.iter() {
            let private_key = derive_spend_key_from_bytes32(test_case.input);
            let pubkey = private_key.to_public_key().0;
            assert!(!pubkey.is_dummy_pubkey());
            assert_eq!(pubkey.to_hex(), test_case.public_key);
        }
    }

    #[test]
    fn test_derive_view_key_from_spend_key() {
        let test_cases = [
            ViewKeyTestCase {
                input: "0x0000000000000000000000000000000000000000000000000000000000000000"
                    .parse()
                    .unwrap(),
                output: "0x1c5420b19f9e87e8c96fac0a0e3a0dfc3d262c029a58ce048f25e0249ad48cf2"
                    .parse()
                    .unwrap(),
            },
            ViewKeyTestCase {
                input: "0x8b7623aee520e739189eaf541558a97e28b413befdc19e0bbaf7002e30cf2a15"
                    .parse()
                    .unwrap(),
                output: "0x0831e6a88f10d27f934113b7b3cfadfea281059e63eac2d4cb71b808added243"
                    .parse()
                    .unwrap(),
            },
        ];
        for test_case in test_cases.iter() {
            let derived_view_key = derive_view_key_from_spend_key(&test_case.input);
            assert_eq!(derived_view_key.to_string(), test_case.output.to_string());
        }
    }
}
