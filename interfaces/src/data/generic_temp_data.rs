use serde::{Deserialize, Serialize};

use super::encryption::Encryption;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenericTempData {
    pub data: Vec<u8>,
}

impl Encryption for GenericTempData {}
