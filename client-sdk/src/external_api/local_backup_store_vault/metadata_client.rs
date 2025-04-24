use csv;
use intmax2_interfaces::data::meta_data::MetaData;
use intmax2_zkp::ethereum_types::{u256::U256, u32limb_trait::U32LimbTrait};
use itertools::Itertools;
use std::path::PathBuf;

use super::error::IOError;

#[derive(Clone, Debug)]
pub struct MetaDataClient {
    root_path: PathBuf,
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
        file_path.push("metadata");
        file_path.set_extension("csv");
        file_path
    }

    pub fn read(&self, topic: &str, pubkey: U256) -> Result<Vec<MetaData>, IOError> {
        let file_path = self.file_path(topic, pubkey);
        if !file_path.exists() {
            return Ok(vec![]);
        }
        let file_content =
            std::fs::read_to_string(&file_path).map_err(|e| IOError::ReadError(e.to_string()))?;
        let mut reader = csv::Reader::from_reader(file_content.as_bytes());
        let mut records = Vec::new();
        for result in reader.deserialize() {
            let record: MetaData = result.map_err(|e| IOError::ParseError(e.to_string()))?;
            records.push(record);
        }
        Ok(records)
    }

    pub fn append(&self, topic: &str, pubkey: U256, records: &[MetaData]) -> Result<(), IOError> {
        let read_records = self.read(topic, pubkey)?;
        let mut all_records = records
            .iter()
            .chain(read_records.iter())
            .cloned()
            .dedup() // deduplicate records
            .collect::<Vec<_>>();
        all_records.sort();
        self.write(topic, pubkey, &all_records)?;
        Ok(())
    }

    fn write(&self, topic: &str, pubkey: U256, records: &[MetaData]) -> Result<(), IOError> {
        let dir_path = self.dir_path(topic, pubkey);
        if !dir_path.exists() {
            std::fs::create_dir_all(&dir_path)
                .map_err(|e| IOError::CreateDirAllError(e.to_string()))?;
        }
        let file_path = self.file_path(topic, pubkey);
        let mut writer = csv::Writer::from_writer(vec![]);
        for record in records {
            writer
                .serialize(record)
                .map_err(|e| IOError::WriteError(e.to_string()))?;
        }
        let csv_bytes = writer
            .into_inner()
            .map_err(|e| IOError::WriteError(e.to_string()))?;
        std::fs::write(&file_path, &csv_bytes).map_err(|e| IOError::WriteError(e.to_string()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use intmax2_zkp::ethereum_types::{bytes32::Bytes32, u256::U256};

    #[test]
    fn test_metadata_client() {
        let root_path = PathBuf::from("test_data");
        let client = MetaDataClient::new(root_path.clone());

        let topic = "test_topic";
        let pubkey = U256::from(12346);
        let digest = Bytes32::from_hex("0xbeef").unwrap();
        let timestamp = 1234567890;

        // Write metadata
        let meta = MetaData { digest, timestamp };
        client.append(topic, pubkey, &[meta]).unwrap();
    }
}
