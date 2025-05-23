use std::{fmt::Display, str::FromStr};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TransferType {
    Normal,
    Withdrawal,
    TransferFee,
    TransferCollateralFee,
    WithdrawalFee,
    ClaimFee,
}

impl Display for TransferType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransferType::Normal => write!(f, "Normal"),
            TransferType::Withdrawal => write!(f, "Withdrawal"),
            TransferType::TransferFee => write!(f, "TransferFee"),
            TransferType::TransferCollateralFee => write!(f, "TransferCollateralFee"),
            TransferType::WithdrawalFee => write!(f, "WithdrawalFee"),
            TransferType::ClaimFee => write!(f, "ClaimFee"),
        }
    }
}

impl FromStr for TransferType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Normal" => Ok(TransferType::Normal),
            "Withdrawal" => Ok(TransferType::Withdrawal),
            "TransferFee" => Ok(TransferType::TransferFee),
            "TransferCollateralFee" => Ok(TransferType::TransferCollateralFee),
            "WithdrawalFee" => Ok(TransferType::WithdrawalFee),
            "ClaimFee" => Ok(TransferType::ClaimFee),
            _ => Err(format!("Invalid transfer type: {s}")),
        }
    }
}
