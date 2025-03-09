use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};

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
            DataType::Transfer => "v1/ao/transfer".to_string(),
            DataType::Deposit => "v1/ao/deposit".to_string(),
            DataType::Withdrawal => "v1/aa/withdrawal".to_string(),
            DataType::Tx => "v1/aa/tx".to_string(),
            DataType::SenderProofSet => "v1/aa/sender_proof_set".to_string(),
            DataType::UserData => "v1/aa/user_data".to_string(),
        }
    }

    pub fn from_topic(topic: &str) -> Result<Self, String> {
        match topic {
            "v1/ao/transfer" => Ok(DataType::Transfer),
            "v1/ao/deposit" => Ok(DataType::Deposit),
            "v1/aa/withdrawal" => Ok(DataType::Withdrawal),
            "v1/aa/tx" => Ok(DataType::Tx),
            "v1/aa/sender_proof_set" => Ok(DataType::SenderProofSet),
            "v1/aa/user_data" => Ok(DataType::UserData),
            _ => Err(format!("Invalid topic: {}", topic)),
        }
    }
}
