use intmax2_zkp::ethereum_types::{bytes32::Bytes32, u32limb_trait::U32LimbTrait as _};
use sha2::{Digest as _, Sha256};

pub fn get_digest(input: &[u8]) -> Bytes32 {
    let digest = Sha256::digest(input);
    Bytes32::from_bytes_be(digest.as_slice()).unwrap()
}
