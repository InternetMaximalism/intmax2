//! These functions reference reth.
//! <https://github.com/paradigmxyz/reth/blob/main/crates/net/ecies/src/algorithm.rs>
//! <https://github.com/paradigmxyz/reth/blob/main/crates/net/ecies/src/util.rs>

use alloy_primitives::B256;
use ark_bn254::{g1::G1Affine, Fr, G1Projective};
use ark_ec::{AffineRepr, CurveGroup};
use ark_std::Zero;
use hmac::{Hmac, Mac};
use intmax2_zkp::ethereum_types::{u256::U256, u32limb_trait::U32LimbTrait};
use plonky2_bn254::fields::{recover::RecoverFromX, sgn::Sgn};
use sha2::{Digest, Sha256};
use std::ops::Neg;

pub(crate) const U256_SIZE: usize = 32;

pub(crate) fn sha256(data: &[u8]) -> B256 {
    B256::from(Sha256::digest(data).as_ref())
}

pub(crate) fn hmac_sha256(key: &[u8], input: &[&[u8]], auth_data: &[u8]) -> B256 {
    let mut hmac = Hmac::<Sha256>::new_from_slice(key).unwrap();
    for input in input {
        hmac.update(input);
    }
    hmac.update(auth_data);
    B256::from_slice(&hmac.finalize().into_bytes())
}

pub(crate) fn ecdh_xy(remote_public_key_x: &U256, secret_key: &Fr) -> (U256, bool) {
    let pubkey_x = *remote_public_key_x;
    let pubkey_g1 = G1Affine::recover_from_x(pubkey_x.into());
    let ecdh_key = G1Affine::from(pubkey_g1 * secret_key);

    (U256::from(*ecdh_key.x().unwrap()), ecdh_key.y.sgn())
}

pub(crate) fn ecdh_x(remote_public_key_x: &U256, secret_key: &Fr) -> U256 {
    ecdh_xy(remote_public_key_x, secret_key).0
}

pub(crate) fn aggregate_ecdh_x(ecdh_shares_xy: &[(U256, bool)]) -> U256 {
    if ecdh_shares_xy.is_empty() {
        panic!("ecdh_shares is empty");
    }

    let mut ecdh_share = G1Projective::zero();
    for (share, y_parity) in ecdh_shares_xy {
        let point = G1Affine::recover_from_x((*share).into());
        ecdh_share += if *y_parity { point.neg() } else { point };
    }

    U256::from(ecdh_share.into_affine().x)
}

pub(crate) fn kdf(secret: U256, s1: &[u8], dest: &mut [u8]) {
    let secret: Vec<u8> = secret.to_bytes_be();
    concat_kdf::derive_key_into::<Sha256>(&secret, s1, dest).unwrap();
}
