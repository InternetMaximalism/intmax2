use base58_monero::base58;
use intmax2_zkp::ethereum_types::{u32limb_trait::U32LimbTrait, EthereumTypeError};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use tiny_keccak::{Hasher as _, Keccak};

use crate::utils::{
    key::{KeyPair, PublicKey, PublicKeyPair, ViewPair},
    network::{self, Network},
    payment_id::PaymentId,
};
use std::{fmt, str::FromStr};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    /// Invalid address magic byte.
    #[error("Invalid magic byte")]
    InvalidMagicByte,
    /// Invalid payment id.
    #[error("Invalid payment ID")]
    InvalidPaymentId,
    /// Mismatch address checksums.
    #[error("Invalid checksum")]
    InvalidChecksum,
    /// Generic invalid format.
    #[error("Invalid format")]
    InvalidFormat,
    /// Monero base58 error.
    #[error("Base58 error: {0}")]
    Base58(#[from] base58::Error),
    /// Network error.
    #[error("Network error: {0}")]
    Network(#[from] network::Error),
    /// Encode error.
    #[error("Encode error: {0}")]
    Encoding(&'static str),

    #[error(transparent)]
    EthereumTypeError(#[from] EthereumTypeError),
}

#[derive(Default, Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub enum AddressType {
    /// Standard address.
    #[default]
    Standard,
    /// Address with a short 8 bytes payment id.
    Integrated(PaymentId),
}

impl AddressType {
    /// Recover the address type given an address bytes and the network.
    pub fn from_slice(bytes: &[u8], net: Network) -> Result<AddressType, Error> {
        if bytes.is_empty() {
            return Err(Error::Encoding(
                "Not enough bytes to decode the AddressType",
            ));
        }
        let byte = bytes[0];
        use AddressType::*;
        use Network::*;
        match net {
            Mainnet => match byte {
                246 => Ok(Standard),
                247 => {
                    if bytes.len() < 73 {
                        return Err(Error::Encoding(
                            "from_slice: Not enough bytes to decode the AddressType (<73)",
                        ));
                    }
                    let payment_id = PaymentId::from_bytes_be(&bytes[65..73])?;
                    Ok(Integrated(payment_id))
                }
                _ => Err(Error::InvalidMagicByte),
            },
            Testnet => match byte {
                180 => Ok(Standard),
                181 => {
                    if bytes.len() < 73 {
                        return Err(Error::Encoding(
                            "from_slice: Not enough bytes to decode the AddressType (<73)",
                        ));
                    }
                    let payment_id = PaymentId::from_bytes_be(&bytes[65..73])?;
                    Ok(Integrated(payment_id))
                }
                _ => Err(Error::InvalidMagicByte),
            },
            Stagenet => match byte {
                192 => Ok(Standard),
                193 => {
                    if bytes.len() < 73 {
                        return Err(Error::Encoding(
                            "from_slice: Not enough bytes to decode the AddressType (<73)",
                        ));
                    }
                    let payment_id = PaymentId::from_bytes_be(&bytes[65..73])?;
                    Ok(Integrated(payment_id))
                }
                _ => Err(Error::InvalidMagicByte),
            },
        }
    }
}

/// A complete Monero typed address valid for a specific network.
#[derive(Default, Debug, PartialEq, Eq, Hash, Copy, Clone)]
pub struct IntmaxAddress {
    pub network: Network,
    pub addr_type: AddressType,
    pub public_spend: PublicKey,
    pub public_view: PublicKey,
}

impl IntmaxAddress {
    pub fn standard(
        network: Network,
        public_spend: PublicKey,
        public_view: PublicKey,
    ) -> IntmaxAddress {
        IntmaxAddress {
            network,
            addr_type: AddressType::Standard,
            public_spend,
            public_view,
        }
    }

    pub fn integrated(
        network: Network,
        public_spend: PublicKey,
        public_view: PublicKey,
        payment_id: PaymentId,
    ) -> IntmaxAddress {
        IntmaxAddress {
            network,
            addr_type: AddressType::Integrated(payment_id),
            public_spend,
            public_view,
        }
    }

    pub fn from_viewpair(network: Network, keys: &ViewPair) -> IntmaxAddress {
        let public_view = PublicKey::from_private_key(&keys.view);
        IntmaxAddress {
            network,
            addr_type: AddressType::Standard,
            public_spend: keys.spend,
            public_view,
        }
    }

    pub fn from_keypair(network: Network, keys: &KeyPair) -> IntmaxAddress {
        let public_spend = PublicKey::from_private_key(&keys.spend);
        let public_view = PublicKey::from_private_key(&keys.view);
        IntmaxAddress {
            network,
            addr_type: AddressType::Standard,
            public_spend,
            public_view,
        }
    }

    pub fn from_public_keypair(network: Network, keys: &PublicKeyPair) -> IntmaxAddress {
        IntmaxAddress {
            network,
            addr_type: AddressType::Standard,
            public_spend: keys.spend,
            public_view: keys.view,
        }
    }

    /// Parse an address from a vector of bytes, fail if the magic byte is incorrect, if public
    /// keys are not valid points, if payment id is invalid, and if checksums mismatch.
    pub fn from_bytes(bytes: &[u8]) -> Result<IntmaxAddress, Error> {
        if bytes.is_empty() || bytes.len() < 65 {
            return Err(Error::Encoding(
                "from_bytes: Not enough bytes to decode the Address (<65)",
            ));
        }
        let network = Network::from_u8(bytes[0])?;
        let addr_type = AddressType::from_slice(bytes, network)?;
        let public_spend =
            PublicKey::from_slice(&bytes[1..33]).map_err(|_| Error::InvalidFormat)?;
        let public_view =
            PublicKey::from_slice(&bytes[33..65]).map_err(|_| Error::InvalidFormat)?;

        let (checksum_bytes, checksum) = match addr_type {
            AddressType::Standard => {
                if bytes.len() < 69 {
                    return Err(Error::Encoding(
                        "from_bytes: Not enough bytes to decode the Address (<69)",
                    ));
                }
                (&bytes[0..65], &bytes[65..69])
            }
            AddressType::Integrated(_) => {
                if bytes.len() < 77 {
                    return Err(Error::Encoding(
                        "from_bytes: Not enough bytes to decode the Address (<77)",
                    ));
                }
                (&bytes[0..73], &bytes[73..77])
            }
        };
        let verify_checksum = keccak256(checksum_bytes);
        if &verify_checksum[0..4] != checksum {
            return Err(Error::InvalidChecksum);
        }

        Ok(IntmaxAddress {
            network,
            addr_type,
            public_spend,
            public_view,
        })
    }

    pub fn to_public_keypair(&self) -> PublicKeyPair {
        PublicKeyPair {
            spend: self.public_spend,
            view: self.public_view,
        }
    }

    pub fn as_bytes(&self) -> Vec<u8> {
        let mut bytes = vec![self.network.as_u8(&self.addr_type)];
        bytes.extend_from_slice(&self.public_spend.to_bytes());
        bytes.extend_from_slice(&self.public_view.to_bytes());
        if let AddressType::Integrated(payment_id) = &self.addr_type {
            bytes.extend_from_slice(&payment_id.to_bytes_be());
        }
        let checksum = keccak256(bytes.as_slice());
        bytes.extend_from_slice(&checksum[0..4]);
        bytes
    }
}

impl fmt::Display for IntmaxAddress {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}",
            base58::encode(self.as_bytes().as_slice()).map_err(|_| fmt::Error)?
        )
    }
}

impl FromStr for IntmaxAddress {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_bytes(&base58::decode(s)?)
    }
}

impl Serialize for IntmaxAddress {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for IntmaxAddress {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        IntmaxAddress::from_str(&s).map_err(serde::de::Error::custom)
    }
}

pub fn keccak256(input: &[u8]) -> [u8; 32] {
    let mut keccak = Keccak::v256();
    let mut out = [0u8; 32];
    keccak.update(input);
    keccak.finalize(&mut out);
    out
}

#[cfg(test)]
mod tests {
    use crate::utils::key::PrivateKey;

    use super::*;

    #[test]
    fn test_address_conversion() {
        let mut rng = &mut rand::thread_rng();
        let view = PrivateKey::rand(&mut rng);
        let spend = PrivateKey::rand(&mut rng);
        let keys = KeyPair { view, spend };
        let addr = IntmaxAddress::from_keypair(Network::Mainnet, &keys);
        let address_str = addr.to_string();
        let recovered_addr = IntmaxAddress::from_str(&address_str).unwrap();
        assert_eq!(addr, recovered_addr);
    }
}
