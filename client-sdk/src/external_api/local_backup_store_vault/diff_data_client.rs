use super::error::IOError;
use intmax2_zkp::ethereum_types::bytes32::Bytes32;
use serde::{Deserialize, Serialize};
use serde_with::{base64::Base64, serde_as};
use std::path::PathBuf;

#[serde_as]
#[derive(Debug, Deserialize, Serialize)]
pub struct DiffRecord {
    pub topic: String,
    pub pubkey: Bytes32,
    pub digest: Bytes32,
    pub timestamp: u64,
    #[serde_as(as = "Base64")]
    pub data: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct DiffDataClient;

impl DiffDataClient {
    pub fn read(&self, file_path: PathBuf) -> Result<Vec<DiffRecord>, IOError> {
        let file_content =
            std::fs::read_to_string(&file_path).map_err(|e| IOError::ReadError(e.to_string()))?;
        let mut reader = csv::Reader::from_reader(file_content.as_bytes());
        let mut records = Vec::new();
        for result in reader.deserialize() {
            let record: DiffRecord = result.map_err(|e| IOError::ParseError(e.to_string()))?;
            records.push(record);
        }
        Ok(records)
    }
}
