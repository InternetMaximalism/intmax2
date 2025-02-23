use std::{fmt, str::FromStr};

use async_trait::async_trait;
use intmax2_zkp::{
    common::signature::key_set::KeySet,
    ethereum_types::{bytes32::Bytes32, u256::U256},
};
use serde::{Deserialize, Serialize};
use serde_with::{base64::Base64, serde_as};

use crate::api::error::ServerError;

use super::types::{DataWithMetaData, MetaDataCursor, MetaDataCursorResponse};

pub const MAX_BATCH_SIZE: usize = 256;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum DataType {
    Deposit,
    Transfer,
    Withdrawal,
    Tx,
}

impl DataType {
    // Returns true if the data type requires authentication when saving.
    pub fn need_auth(&self) -> bool {
        match self {
            DataType::Deposit => false,
            DataType::Transfer => false,
            DataType::Withdrawal => true,
            DataType::Tx => true,
        }
    }
}

impl fmt::Display for DataType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let t = match self {
            DataType::Deposit => "deposit".to_string(),
            DataType::Transfer => "transfer".to_string(),
            DataType::Withdrawal => "withdrawal".to_string(),
            DataType::Tx => "tx".to_string(),
        };
        write!(f, "{}", t)
    }
}

impl FromStr for DataType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "deposit" => Ok(DataType::Deposit),
            "transfer" => Ok(DataType::Transfer),
            "withdrawal" => Ok(DataType::Withdrawal),
            "tx" => Ok(DataType::Tx),
            _ => Err(format!("Invalid data type: {}", s)),
        }
    }
}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveDataEntry {
    pub data_type: DataType,
    pub pubkey: U256,
    #[serde_as(as = "Base64")]
    pub encrypted_data: Vec<u8>,
}

#[async_trait(?Send)]
pub trait StoreVaultClientInterface {
    async fn save_user_data(
        &self,
        key: KeySet,
        prev_digest: Option<Bytes32>,
        encrypted_data: &[u8],
    ) -> Result<(), ServerError>;

    async fn get_user_data(&self, key: KeySet) -> Result<Option<Vec<u8>>, ServerError>;

    async fn save_sender_proof_set(
        &self,
        ephemeral_key: KeySet,
        encrypted_data: &[u8],
    ) -> Result<(), ServerError>;

    async fn get_sender_proof_set(&self, ephemeral_key: KeySet) -> Result<Vec<u8>, ServerError>;

    async fn save_data_batch(
        &self,
        key: KeySet,
        entries: &[SaveDataEntry],
    ) -> Result<Vec<String>, ServerError>;

    async fn get_data_batch(
        &self,
        key: KeySet,
        data_type: DataType,
        uuids: &[String],
    ) -> Result<Vec<DataWithMetaData>, ServerError>;

    async fn get_data_sequence(
        &self,
        key: KeySet,
        data_type: DataType,
        cursor: &MetaDataCursor,
    ) -> Result<(Vec<DataWithMetaData>, MetaDataCursorResponse), ServerError>;

    async fn save_misc(
        &self,
        key: KeySet,
        topic: Bytes32,
        encrypted_data: &[u8],
    ) -> Result<String, ServerError>;

    async fn get_misc_sequence(
        &self,
        key: KeySet,
        topic: Bytes32,
        cursor: &MetaDataCursor,
    ) -> Result<(Vec<DataWithMetaData>, MetaDataCursorResponse), ServerError>;
}

#[cfg(test)]
mod tests {
    use super::DataType;
    use std::str::FromStr;

    #[test]
    fn test_data_type() {
        let deposit = DataType::from_str("deposit").unwrap();
        assert_eq!(deposit.to_string(), "deposit");

        let withdrawal = DataType::Withdrawal;
        assert_eq!(withdrawal.to_string(), "withdrawal");
    }
}
