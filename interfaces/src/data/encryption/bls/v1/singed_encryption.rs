use intmax2_zkp::common::signature::flatten::FlatG2;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct SignedEncryption {
    pub data: Vec<u8>,
    pub signature: Option<FlatG2>,
}

impl SignedEncryption {
    pub fn new(data: Vec<u8>, signature: Option<FlatG2>) -> Self {
        Self { data, signature }
    }
}
