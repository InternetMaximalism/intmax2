use intmax2_zkp::ethereum_types::{bytes32::Bytes32, u32limb_trait::U32LimbTrait as _};
use sha2::Digest as _;

pub mod payment_memo;

pub fn get_topic(input: &str) -> Bytes32 {
    let digest: [u8; 32] = sha2::Sha256::digest(input).into();
    Bytes32::from_bytes_be(&digest)
}
