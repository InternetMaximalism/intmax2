use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};

use super::{
    rw_rights::{ReadRights, WriteRights},
    topic::topic_from_rights,
};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum DataType {
    Deposit,
    Transfer,
    Withdrawal,
    Tx,
    UserData,
    SenderProofSet,
}

impl fmt::Display for DataType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let t = match self {
            DataType::Deposit => "deposit".to_string(),
            DataType::Transfer => "transfer".to_string(),
            DataType::Withdrawal => "withdrawal".to_string(),
            DataType::Tx => "tx".to_string(),
            DataType::SenderProofSet => "sender_proof_set".to_string(),
            DataType::UserData => "user_data".to_string(),
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
            "sender_proof_set" => Ok(DataType::SenderProofSet),
            _ => Err(format!("Invalid data type: {}", s)),
        }
    }
}

impl DataType {
    pub fn to_topic(&self) -> String {
        match self {
            DataType::Transfer => {
                topic_from_rights(ReadRights::AuthRead, WriteRights::OpenWrite, "transfer")
            }
            DataType::Deposit => {
                topic_from_rights(ReadRights::AuthRead, WriteRights::OpenWrite, "deposit")
            }
            DataType::Withdrawal => {
                topic_from_rights(ReadRights::AuthRead, WriteRights::AuthWrite, "withdrawal")
            }
            DataType::Tx => topic_from_rights(ReadRights::AuthRead, WriteRights::AuthWrite, "tx"),
            DataType::SenderProofSet => topic_from_rights(
                ReadRights::OpenRead,
                WriteRights::SingleOpenWrite,
                "sender_proof_set",
            ),
            DataType::UserData => {
                topic_from_rights(ReadRights::AuthRead, WriteRights::AuthWrite, "user_data")
            }
        }
    }
}
