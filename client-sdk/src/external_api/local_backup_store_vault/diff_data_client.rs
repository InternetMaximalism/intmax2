use super::error::IOError;
use csv::WriterBuilder;
use intmax2_interfaces::{
    api::store_vault_server::interface::SaveDataEntry, utils::digest::get_digest,
};
use intmax2_zkp::ethereum_types::bytes32::Bytes32;
use serde::{Deserialize, Serialize};
use serde_with::{base64::Base64, serde_as};
use std::path::Path;

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
    pub fn read(&self, file_path: &Path) -> Result<Vec<DiffRecord>, IOError> {
        let file_content =
            std::fs::read_to_string(file_path).map_err(|e| IOError::ReadError(e.to_string()))?;
        let mut reader = csv::Reader::from_reader(file_content.as_bytes());
        let mut records = Vec::new();
        for result in reader.deserialize() {
            let record: DiffRecord = result.map_err(|e| IOError::ParseError(e.to_string()))?;
            records.push(record);
        }
        Ok(records)
    }
}

pub fn make_backup_csv_from_entries(entries: &[SaveDataEntry]) -> Result<String, IOError> {
    let mut records = Vec::new();
    for entry in entries {
        let record = DiffRecord {
            topic: entry.topic.clone(),
            pubkey: entry.pubkey.into(),
            digest: get_digest(&entry.data),
            timestamp: chrono::Utc::now().timestamp() as u64,
            data: entry.data.clone(),
        };
        records.push(record);
    }
    make_backup_csv_from_records(&records)
}

pub fn make_backup_csv_from_records(records: &[DiffRecord]) -> Result<String, IOError> {
    let mut wtr = WriterBuilder::new().from_writer(vec![]);
    for record in records {
        wtr.serialize(record)
            .map_err(|e| IOError::SerializeError(e.to_string()))?;
    }
    let csv_bytes = wtr
        .into_inner()
        .map_err(|e| IOError::WriteError(e.to_string()))?;
    let csv_content = String::from_utf8(csv_bytes).unwrap();
    Ok(csv_content)
}
