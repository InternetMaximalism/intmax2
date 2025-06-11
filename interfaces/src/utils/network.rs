use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::utils::address::AddressType;

/// Potential errors encountered while manipulating Intmax networks.
#[derive(Error, Debug, PartialEq, Eq)]
pub enum Error {
    /// Invalid magic network byte.
    #[error("Invalid magic network byte")]
    InvalidMagicByte,
}

#[derive(Default, Debug, PartialEq, Eq, Hash, Clone, Copy, Serialize, Deserialize)]
pub enum Network {
    #[default]
    Mainnet,
    Stagenet,
    Testnet,
}

impl Network {
    /// Get the associated magic byte given an address type.
    pub fn as_u8(self, addr_type: &AddressType) -> u8 {
        match self {
            Network::Mainnet => match addr_type {
                // starts from "i" when encoded in base58
                AddressType::Standard => 246,
                AddressType::Integrated(_) => 247,
            },
            Network::Stagenet => match addr_type {
                // starts from "T" when encoded in base58
                AddressType::Standard => 156,
                AddressType::Integrated(_) => 157,
            },
            Network::Testnet => match addr_type {
                // starts from "X" when encoded in base58
                AddressType::Standard => 180,
                AddressType::Integrated(_) => 181,
            },
        }
    }

    pub fn from_u8(byte: u8) -> Result<Network, Error> {
        match byte {
            246 | 247 => Ok(Network::Mainnet),
            156 | 157 => Ok(Network::Stagenet),
            180 | 181 => Ok(Network::Testnet),
            _ => Err(Error::InvalidMagicByte),
        }
    }
}

impl fmt::Display for Network {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Network::Mainnet => write!(f, "mainnet"),
            Network::Stagenet => write!(f, "stagenet"),
            Network::Testnet => write!(f, "testnet"),
        }
    }
}

impl FromStr for Network {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "mainnet" => Ok(Network::Mainnet),
            "stagenet" => Ok(Network::Stagenet),
            "testnet" => Ok(Network::Testnet),
            _ => Err(Error::InvalidMagicByte),
        }
    }
}
