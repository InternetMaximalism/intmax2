use alloy::primitives::B256;
use ark_bn254::Fr;
use intmax2_zkp::{common::signature_content::key_set::KeySet, ethereum_types::u256::U256};
use num_bigint::BigUint;
use num_traits::identities::Zero;
use sha2::{Digest, Sha512};

pub fn generate_intmax_account_from_eth_key(eth_private_key: B256) -> KeySet {
    let mut hasher = Sha512::new();
    loop {
        hasher.update(b"INTMAX");
        hasher.update(eth_private_key.0);
        let digest = hasher.clone().finalize();
        let provisional_private_key: Fr = BigUint::from_bytes_be(&digest).into();
        if provisional_private_key.is_zero() {
            continue;
        }
        let provisional_private_key: U256 =
            BigUint::from(provisional_private_key).try_into().unwrap();
        return KeySet::new(provisional_private_key);
    }
}

#[cfg(test)]
mod test {
    use alloy::primitives::B256;
    use intmax2_zkp::ethereum_types::u32limb_trait::U32LimbTrait;

    use crate::client::key_from_eth::generate_intmax_account_from_eth_key;

    struct TestCase {
        private_key: B256,
        public_key: String,
    }

    #[test]
    fn test_account() {
        let test_cases = [
            TestCase {
                private_key: "f68ff926147a67518161e65cd54a3a44c2379e4b63c74b52cfc74274d2586299"
                    .parse()
                    .unwrap(),
                public_key: "0x2f2ddf326b1b4528706ecab6ff465b15cc1f4a4a2d8ea5d39d66ffb0a91a277c"
                    .to_string(),
            },
            TestCase {
                private_key: "3db985c15e2788a9f03a797c71151571cbbd0cb2a89402f640102cb8b445e59a"
                    .parse()
                    .unwrap(),
                public_key: "0x17aebd78d4259e734ba1c9ce1b58c9adea5ab3e68c61e6251dd3016085101941"
                    .to_string(),
            },
            TestCase {
                private_key: "962bc2ea6e76fc3863906a894f3b17cce375ff298c7c5efcf0d4ce9d054e7e4e"
                    .parse()
                    .unwrap(),
                public_key: "0x1fb62949642c57749922484377541e70445881599cfb19c74066fe0f885510af"
                    .to_string(),
            },
            TestCase {
                private_key: "25be37b3ca8370a172765133f23c849905f21ed2dd90422bc8901cbbe69e3e1c"
                    .parse()
                    .unwrap(),
                public_key: "0x2c8ffeb9b3a365c0387f841973defbb203be92a509f075a0821aaeec79f7080f"
                    .to_string(),
            },
        ];

        for test_case in test_cases.iter() {
            let account = generate_intmax_account_from_eth_key(test_case.private_key);
            assert!(!account.is_dummy());
            assert_eq!(account.pubkey.to_hex(), test_case.public_key);
        }
    }
}
