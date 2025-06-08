use std::str::FromStr;

use intmax2_zkp::ethereum_types::{
    u32limb_trait::{self, U32LimbTrait},
    EthereumTypeError,
};
use serde::{Deserialize, Serialize};

pub const PAYMENT_ID_LEN: usize = 2;

#[derive(Default, PartialEq, Eq, Hash, Clone, Copy)]
pub struct PaymentId {
    limbs: [u32; PAYMENT_ID_LEN],
}

impl core::fmt::Debug for PaymentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

impl core::fmt::Display for PaymentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        core::fmt::Debug::fmt(&self, f)
    }
}

impl FromStr for PaymentId {
    type Err = EthereumTypeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_hex(s).map_err(|e| {
            EthereumTypeError::HexParseError(format!("Failed to parse PaymentId: {}", e))
        })
    }
}

impl Serialize for PaymentId {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for PaymentId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        let value = Self::from_hex(&s).map_err(serde::de::Error::custom)?;
        Ok(value)
    }
}

impl U32LimbTrait<PAYMENT_ID_LEN> for PaymentId {
    fn to_u32_vec(&self) -> Vec<u32> {
        self.limbs.to_vec()
    }

    fn from_u32_slice(limbs: &[u32]) -> u32limb_trait::Result<Self> {
        if limbs.len() != PAYMENT_ID_LEN {
            return Err(EthereumTypeError::InvalidLengthSimple(limbs.len()));
        }
        Ok(Self {
            limbs: limbs.try_into().unwrap(),
        })
    }
}
