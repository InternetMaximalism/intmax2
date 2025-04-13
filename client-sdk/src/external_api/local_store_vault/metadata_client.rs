use std::path::PathBuf;

use csv;
use intmax2_interfaces::data::meta_data::MetaData;
use intmax2_zkp::ethereum_types::{bytes32::Bytes32, u256::U256, u32limb_trait::U32LimbTrait};
use serde::{Deserialize, Serialize};

use super::error::IOError;

#[derive(Debug, Deserialize, Serialize)]
pub struct MetaDataRecord {
    pub digest: Bytes32,
    pub timestamp: u64,
}

pub struct MetaDataClient {
    pub root_path: PathBuf,
}

impl MetaDataClient {
    pub fn new(root_path: PathBuf) -> Self {
        MetaDataClient { root_path }
    }

    fn dir_path(&self, topic: &str, pubkey: U256) -> PathBuf {
        let mut dir_path = self.root_path.clone();
        dir_path.push(topic);
        dir_path.push(pubkey.to_hex());
        dir_path
    }

    fn file_path(&self, topic: &str, pubkey: U256) -> PathBuf {
        let mut file_path = self.root_path.clone();
        file_path.push(topic);
        file_path.push(pubkey.to_hex());
        file_path.set_extension("csv");
        file_path
    }

    pub fn read(&self, topic: &str, pubkey: U256) -> Result<Vec<MetaDataRecord>, IOError> {
        let file_path = self.file_path(topic, pubkey);
        if !file_path.exists() {
            return Ok(vec![]);
        }
        let mut reader =
            csv::Reader::from_path(file_path).map_err(|e| IOError::ReadError(e.to_string()))?;
        let mut records = Vec::new();
        for result in reader.deserialize() {
            let record: MetaDataRecord = result.map_err(|e| IOError::ParseError(e.to_string()))?;
            records.push(record);
        }
        Ok(records)
    }

    pub fn write(&self, topic: &str, pubkey: U256, records: &[MetaData]) -> Result<(), IOError> {
        let records = records
            .iter()
            .map(|record| MetaDataRecord {
                digest: record.digest,
                timestamp: record.timestamp,
            })
            .collect::<Vec<_>>();
        let read_records = self.read(topic, pubkey)?;
        let all_records = records
            .into_iter()
            .chain(read_records.into_iter())
            .collect::<Vec<_>>();
        let dir_path = self.dir_path(topic, pubkey);
        if !dir_path.exists() {
            std::fs::create_dir_all(&dir_path)
                .map_err(|e| IOError::CreateDirAllError(e.to_string()))?;
        }
        let file_path = self.file_path(topic, pubkey);
        let mut writer =
            csv::Writer::from_path(file_path).map_err(|e| IOError::WriteError(e.to_string()))?;
        for record in all_records {
            writer
                .serialize(record)
                .map_err(|e| IOError::WriteError(e.to_string()))?;
        }
        writer
            .flush()
            .map_err(|e| IOError::WriteError(e.to_string()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use intmax2_zkp::ethereum_types::u256::U256;
    use std::fs;

    #[test]
    fn test_metadata_client() {
        let root_path = PathBuf::from("test_data");
        let client = MetaDataClient::new(root_path.clone());

        let topic = "test_topic";
        let pubkey = U256::from(12345);
        let digest = Bytes32::from_hex("0xbeef").unwrap();
        let timestamp = 1234567890;

        // Write metadata
        let meta = MetaData { digest, timestamp };
        client.write(topic, pubkey, &[meta]).unwrap();

        // Read metadata
        let records = client.read(topic, pubkey).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].digest, digest);
        assert_eq!(records[0].timestamp, timestamp);
    }
}
