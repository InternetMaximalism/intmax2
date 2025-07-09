use alloy::primitives::B256;
use intmax2_client_sdk::external_api::contract::convert::convert_b256_to_bytes32;
use intmax2_zkp::{common::signature_content::key_set::KeySet, ethereum_types::bytes32::Bytes32};

pub fn privkey_to_keyset(privkey: B256) -> KeySet {
    let privkey: Bytes32 = convert_b256_to_bytes32(privkey);
    KeySet::new(privkey.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ark_bn254::{Fr, G1Affine};
    use ark_ec::AffineRepr;
    use num_bigint::BigUint;
    use plonky2_bn254::fields::recover::RecoverFromX as _;

    fn assert_keyset_valid(h: B256) {
        let keyset = privkey_to_keyset(h);

        // Get expected pubkey from privkey
        let privkey_fr: Fr = BigUint::from(keyset.privkey).into();
        let expected_pubkey_g1: G1Affine = (G1Affine::generator() * privkey_fr).into();

        // Ensure pubkey is correct
        assert_eq!(
            keyset.pubkey_g1(),
            expected_pubkey_g1,
            "Public key mismatch for privkey: {h:?}"
        );

        // Ensure pubkey is not dummy
        assert!(
            !keyset.pubkey.is_dummy_pubkey(),
            "Pubkey should not be dummy: {:?}",
            keyset.pubkey
        );

        // Check recovery via x-coordinate
        let recovered = G1Affine::recover_from_x(keyset.pubkey.into());
        assert_eq!(
            recovered,
            keyset.pubkey_g1(),
            "Recovered pubkey from x doesn't match"
        );
    }

    #[test]
    #[should_panic]
    fn test_zero_privkey() {
        let h = B256::ZERO;
        assert_keyset_valid(h);
    }

    #[test]
    #[should_panic(expected = "!pubkey.is_dummy_pubkey()")]
    fn test_one_privkey() {
        let mut bytes = [0u8; 32];
        bytes[31] = 0x01;
        let h = B256::from(bytes);
        assert_keyset_valid(h);
    }

    #[test]
    fn test_max_privkey() {
        let h = B256::from([0xFF; 32]);
        assert_keyset_valid(h);
    }

    #[test]
    fn test_near_max_privkey() {
        let mut bytes = [0xFF; 32];
        bytes[31] = 0xFE;
        let h = B256::from(bytes);
        assert_keyset_valid(h);
    }

    #[test]
    fn test_mid_privkey() {
        let mut bytes = [0u8; 32];
        bytes[0] = 0x80; // MSB = 1, rest = 0
        let h = B256::from(bytes);
        assert_keyset_valid(h);
    }

    #[test]
    fn test_leading_zeros_privkey() {
        let mut bytes = [0u8; 32];
        bytes[30] = 0x01;
        let h = B256::from(bytes);
        assert_keyset_valid(h);
    }
}
