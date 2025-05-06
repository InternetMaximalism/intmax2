//! These functions reference reth.
//! <https://github.com/paradigmxyz/reth/blob/main/crates/net/ecies/src/algorithm.rs>
//! <https://github.com/paradigmxyz/reth/blob/main/crates/net/ecies/src/util.rs>

use alloy::primitives::B256;
use ark_bn254::{g1::G1Affine, G1Projective};
use ark_ec::{AffineRepr, CurveGroup};
use ark_std::{ops::Mul, Zero};
use hmac::{Hmac, Mac};
use intmax2_zkp::{
    common::signature_content::key_set::KeySet,
    ethereum_types::{u256::U256, u32limb_trait::U32LimbTrait},
};
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

pub(crate) fn ecdh_xy(remote_public_key_x: &U256, secret_key: &U256) -> G1Affine {
    let share = KeySet::new(*secret_key);
    let c1 = G1Affine::recover_from_x((*remote_public_key_x).into());
    let xi = share.privkey_fr();

    // C1^xi
    c1.mul(xi).into_affine()
}

pub(crate) fn ecdh_x(remote_public_key_x: &U256, secret_key: &U256) -> U256 {
    let ecdh_key = ecdh_xy(remote_public_key_x, secret_key);

    U256::from(*ecdh_key.x().unwrap())
}

pub(crate) fn aggregate_ecdh_x(ecdh_shares_xy: &[G1Affine]) -> U256 {
    if ecdh_shares_xy.is_empty() {
        panic!("ecdh_shares is empty");
    }

    let mut ecdh_share = G1Projective::zero();
    for point in ecdh_shares_xy {
        ecdh_share += point;
    }

    U256::from(ecdh_share.into_affine().x)
}

pub(crate) fn kdf(secret: U256, s1: &[u8], dest: &mut [u8]) {
    let secret: Vec<u8> = secret.to_bytes_be();
    concat_kdf::derive_key_into::<Sha256>(&secret, s1, dest).unwrap();
}

pub fn g1_point_to_xy(point: G1Affine) -> (U256, bool) {
    (U256::from(*point.x().unwrap()), point.y.sgn())
}

pub fn xy_to_g1_point(x: U256, y_parity: bool) -> G1Affine {
    let point_without_sign = G1Affine::recover_from_x(x.into());
    if y_parity {
        point_without_sign.neg()
    } else {
        point_without_sign
    }
}
