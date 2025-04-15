use std::{fs, path::PathBuf};

use base64::{prelude::BASE64_STANDARD, Engine};
use intmax2_zkp::ethereum_types::{bytes32::Bytes32, u256::U256, u32limb_trait::U32LimbTrait};

use super::error::IOError;

#[derive(Clone, Debug)]
pub struct LocalDataClient {
    pub root_path: PathBuf,
}

impl LocalDataClient {
    pub fn new(root_path: PathBuf) -> Self {
        LocalDataClient { root_path }
    }

    fn dir_path(&self, topic: &str, pubkey: U256) -> PathBuf {
        let mut dir_path = self.root_path.clone();
        dir_path.push(topic);
        dir_path.push(pubkey.to_hex());
        dir_path
    }

    fn file_path(&self, topic: &str, pubkey: U256, digest: Bytes32) -> PathBuf {
        let mut file_path = self.dir_path(topic, pubkey);
        file_path.push(digest.to_hex());
        file_path.set_extension("txt");
        file_path
    }

    pub fn read(
        &self,
        topic: &str,
        pubkey: U256,
        digest: Bytes32,
    ) -> Result<Option<Vec<u8>>, IOError> {
        let file_path = self.file_path(topic, pubkey, digest);
        if !file_path.exists() {
            return Ok(None);
        }
        let data_base64 =
            fs::read_to_string(file_path).map_err(|e| IOError::ReadError(e.to_string()))?;
        let data = BASE64_STANDARD
            .decode(&data_base64)
            .map_err(|e| IOError::ReadError(e.to_string()))?;
        Ok(Some(data))
    }

    pub fn write(
        &self,
        topic: &str,
        pubkey: U256,
        digest: Bytes32,
        data: &[u8],
    ) -> Result<(), IOError> {
        let dir_path = self.dir_path(topic, pubkey);
        if !dir_path.exists() {
            fs::create_dir_all(&dir_path).map_err(|e| IOError::CreateDirAllError(e.to_string()))?;
        }
        let file_path = self.file_path(topic, pubkey, digest);
        if file_path.exists() {
            // If the file already exists, we do not overwrite it.
            return Ok(());
        }
        let data_base64 = BASE64_STANDARD.encode(data);
        fs::write(file_path, data_base64).map_err(|e| IOError::WriteError(e.to_string()))?;
        Ok(())
    }

    pub fn delete_all(&self, topic: &str, pubkey: U256) -> Result<(), IOError> {
        let dir_path = self.dir_path(topic, pubkey);
        if dir_path.exists() {
            fs::remove_dir_all(&dir_path).map_err(|e| IOError::DeleteError(e.to_string()))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use intmax2_zkp::ethereum_types::{bytes32::Bytes32, u256::U256, u32limb_trait::U32LimbTrait};
    use std::path::PathBuf;

    #[test]
    #[ignore]
    fn test_local_data_client() {
        let client = super::LocalDataClient::new(PathBuf::from("test_data"));
        let topic = "test_topic";
        let pubkey = U256::from(123456);
        let digest = Bytes32::from_hex("0xcafe").unwrap();
        let data = vec![1, 2, 3, 4, 5];

        // Write data
        client.write(topic, pubkey, digest, &data).unwrap();

        // Read data
        let read_data = client.read(topic, pubkey, digest).unwrap().unwrap();
        assert_eq!(read_data, data);
    }
}
