use ark_bn254::{Fr, G1Affine};
use ark_ec::{AffineRepr, CurveGroup};
use ark_ff::{UniformRand, Zero};
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use ark_std::{ops::Mul, rand::Rng};
use intmax2_zkp::{common::signature_content::key_set::KeySet, ethereum_types::u256::U256};
use plonky2_bn254::fields::recover::RecoverFromX;
use sha2::{Digest, Sha256};

type Scalar = Fr;

/// Zero-knowledge proof of correct partial decryption (Chaum-Pedersen proof)
#[derive(Clone, Debug, CanonicalSerialize, CanonicalDeserialize)]
pub struct ZKProof {
    /// The commitment a = g^v
    pub a: G1Affine,
    /// The commitment b = C1^v
    pub b: G1Affine,
    /// The challenge response z = v - c*xi
    pub z: Scalar,
}

/// Convert a hash digest to a scalar field element
fn hash_to_scalar(hash: &[u8]) -> Scalar {
    // Take first 32 bytes of hash and interpret as Scalar
    // Note: This is a simplified approach for demonstration
    let mut repr = [0u8; 32];
    repr.copy_from_slice(&hash[0..32]);

    // Convert to scalar (mod order of scalar field)
    let mut acc = Scalar::zero();
    for byte in repr.iter() {
        acc = acc * Scalar::from(256u64) + Scalar::from(*byte as u64);
    }
    acc
}

/// Compute challenge for Chaum-Pedersen proof
fn compute_challenge(
    g: &G1Affine,
    c1: &G1Affine,
    pk_i: &G1Affine,
    di: &G1Affine,
    a: &G1Affine,
    b: &G1Affine,
) -> Scalar {
    let mut hasher = Sha256::new();

    // Hash all values in the proof
    let mut buf = Vec::new();
    g.serialize_uncompressed(&mut buf).unwrap();
    hasher.update(&buf);

    buf.clear();
    c1.serialize_uncompressed(&mut buf).unwrap();
    hasher.update(&buf);

    buf.clear();
    pk_i.serialize_uncompressed(&mut buf).unwrap();
    hasher.update(&buf);

    buf.clear();
    di.serialize_uncompressed(&mut buf).unwrap();
    hasher.update(&buf);

    buf.clear();
    a.serialize_uncompressed(&mut buf).unwrap();
    hasher.update(&buf);

    buf.clear();
    b.serialize_uncompressed(&mut buf).unwrap();
    hasher.update(&buf);

    // Convert hash output to scalar
    let hash = hasher.finalize();
    hash_to_scalar(&hash)
}

pub fn partial_decrypt_with_proof<R: Rng>(
    remote_public_key_x: &U256,
    secret_key: &U256,
    rng: &mut R,
) -> (G1Affine, ZKProof) {
    let share = KeySet::new(*secret_key);
    let c1 = G1Affine::recover_from_x((*remote_public_key_x).into());

    // Calculate Di = C1^xi
    let xi = share.privkey_fr();
    let di = c1.mul(xi).into_affine();

    // Generator point
    let g = G1Affine::generator();

    // Generate Chaum-Pedersen proof that log_g(PKi) = log_C1(Di)
    // Choose random v
    let v = Scalar::rand(rng);

    // Compute commitments
    let a = g.mul(v).into_affine();
    let b = c1.mul(v).into_affine();

    // Compute challenge c = Hash(g, C1, PKi, Di, a, b)
    let pk_i = G1Affine::recover_from_x(share.pubkey.into());
    let c = compute_challenge(&g, &c1, &pk_i, &di, &a, &b);

    // Compute response z = v - c * xi
    let z = v - c * xi;

    let proof = ZKProof { a, b, z };

    (di, proof)
}

/// Verify a partial decryption share using its zero-knowledge proof
pub fn verify_share(
    pk_share: U256,
    remote_public_key_x: U256,
    di: &G1Affine,
    proof: &ZKProof,
) -> bool {
    // Generator point
    let g = G1Affine::generator();

    // Recompute challenge c = Hash(g, C1, PKi, Di, a, b)
    let c1 = G1Affine::recover_from_x(remote_public_key_x.into());
    let pk_i = G1Affine::recover_from_x(pk_share.into());
    let c = compute_challenge(&g, &c1, &pk_i, di, &proof.a, &proof.b);

    // Verify g^z * PKi^c = a
    let g_z = g.mul(proof.z);
    let pk_i_c = pk_i.mul(c);
    let left1 = (g_z + pk_i_c).into_affine();

    // Verify C1^z * Di^c = b
    let c1_z = c1.mul(proof.z);
    let di_c = di.mul(c);
    let left2 = (c1_z + di_c).into_affine();

    // Check if both equations hold
    left1 == proof.a && left2 == proof.b
}

#[cfg(test)]
mod test {
    use ark_ff::One;
    use intmax2_zkp::common::signature_content::key_set::KeySet;

    use super::*;

    #[test]
    fn test_partial_decrypt_with_proof() {
        let mut rng = rand::thread_rng();
        let server_key = KeySet::rand(&mut rng);
        let client_key = KeySet::rand(&mut rng);

        let (di, proof) =
            partial_decrypt_with_proof(&server_key.pubkey, &client_key.privkey, &mut rng);

        assert!(verify_share(
            client_key.pubkey,
            server_key.pubkey,
            &di,
            &proof
        ));
    }

    #[test]
    fn test_partial_decrypt_with_proof_fail() {
        let mut rng = rand::thread_rng();
        let server_key = KeySet::rand(&mut rng);
        let client_key = KeySet::rand(&mut rng);

        let (di, proof) =
            partial_decrypt_with_proof(&server_key.pubkey, &client_key.privkey, &mut rng);

        // Modify the proof to make it invalid
        let mut invalid_proof = proof.clone();
        invalid_proof.z += Scalar::one();

        assert!(!verify_share(
            client_key.pubkey,
            server_key.pubkey,
            &di,
            &invalid_proof
        ));
    }

    #[test]
    fn test_partial_decrypt_with_proof_invalid_server_key() {
        let mut rng = rand::thread_rng();
        let server_key = KeySet::rand(&mut rng);
        let client_key = KeySet::rand(&mut rng);

        let (di, proof) =
            partial_decrypt_with_proof(&server_key.pubkey, &client_key.privkey, &mut rng);

        // Modify the server key to make it invalid
        let invalid_server_key = KeySet::rand(&mut rng);
        assert!(!verify_share(
            client_key.pubkey,
            invalid_server_key.pubkey,
            &di,
            &proof
        ));
    }
}
