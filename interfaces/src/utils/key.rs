use core::fmt;
use std::str::FromStr;

use intmax2_zkp::{
    common::signature_content::key_set::KeySet,
    ethereum_types::{u256::U256, u32limb_trait::U32LimbTrait},
};
use rand::Rng;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Invalid length")]
    InvalidLength,

    #[error(transparent)]
    EthereumTypeError(#[from] intmax2_zkp::ethereum_types::EthereumTypeError),
}

#[derive(Debug, PartialEq, Eq, Copy, Clone, Hash)]
pub struct PublicKey(pub U256);

impl PublicKey {
    pub fn to_bytes(self) -> [u8; 32] {
        self.0.to_bytes_be().try_into().unwrap()
    }

    pub fn from_slice(data: &[u8]) -> Result<PublicKey, Error> {
        if data.len() != 32 {
            return Err(Error::InvalidLength);
        }
        Ok(PublicKey(U256::from_bytes_be(data)?))
    }

    pub fn from_private_key(privkey: &PrivateKey) -> PublicKey {
        let key = KeySet::new(privkey.0);
        PublicKey(key.pubkey)
    }
}

impl fmt::Display for PublicKey {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0.to_hex())
    }
}

impl FromStr for PublicKey {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let inner = U256::from_hex(s)?;
        Ok(PublicKey(inner))
    }
}

impl TryFrom<&[u8]> for PublicKey {
    type Error = Error;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        Self::from_slice(value)
    }
}

impl Serialize for PublicKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for PublicKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        PublicKey::from_str(&s).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, PartialEq, Eq, Copy, Clone, Hash)]
pub struct PrivateKey(pub U256);

impl PrivateKey {
    pub fn to_bytes(self) -> [u8; 32] {
        self.0.to_bytes_be().try_into().unwrap()
    }

    pub fn from_slice(data: &[u8]) -> Result<PrivateKey, Error> {
        if data.len() != 32 {
            return Err(Error::InvalidLength);
        }
        Ok(PrivateKey(U256::from_bytes_be(data)?))
    }

    pub fn to_public_key(self) -> PublicKey {
        PublicKey::from_private_key(&self)
    }

    pub fn to_key_set(self) -> KeySet {
        KeySet::new(self.0)
    }

    pub fn rand<R: Rng>(rng: &mut R) -> PrivateKey {
        PrivateKey(U256::rand(rng))
    }
}

impl fmt::Display for PrivateKey {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0.to_hex())
    }
}

impl FromStr for PrivateKey {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let inner = U256::from_hex(s)?;
        Ok(PrivateKey(inner))
    }
}

impl TryFrom<&[u8]> for PrivateKey {
    type Error = Error;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        Self::from_slice(value)
    }
}

#[derive(Debug, PartialEq, Eq, Copy, Clone, Hash)]
pub struct KeyPair {
    pub view: PrivateKey,
    pub spend: PrivateKey,
}

#[derive(Debug, PartialEq, Eq, Copy, Clone, Hash)]
pub struct ViewPair {
    pub view: PrivateKey,
    pub spend: PublicKey,
}

impl From<KeyPair> for ViewPair {
    fn from(k: KeyPair) -> ViewPair {
        let spend = PublicKey::from_private_key(&k.spend);
        ViewPair {
            view: k.view,
            spend,
        }
    }
}

impl From<&KeyPair> for ViewPair {
    fn from(k: &KeyPair) -> ViewPair {
        let spend = PublicKey::from_private_key(&k.spend);
        ViewPair {
            view: k.view,
            spend,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Copy, Clone, Hash, Serialize, Deserialize)]
pub struct PublicKeyPair {
    pub view: PublicKey,
    pub spend: PublicKey,
}

impl From<ViewPair> for PublicKeyPair {
    fn from(k: ViewPair) -> PublicKeyPair {
        let view = PublicKey::from_private_key(&k.view);
        PublicKeyPair {
            view,
            spend: k.spend,
        }
    }
}

impl From<&ViewPair> for PublicKeyPair {
    fn from(k: &ViewPair) -> PublicKeyPair {
        let view = PublicKey::from_private_key(&k.view);
        PublicKeyPair {
            view,
            spend: k.spend,
        }
    }
}

impl From<KeyPair> for PublicKeyPair {
    fn from(k: KeyPair) -> PublicKeyPair {
        PublicKeyPair {
            view: PublicKey::from_private_key(&k.view),
            spend: PublicKey::from_private_key(&k.spend),
        }
    }
}

impl From<&KeyPair> for PublicKeyPair {
    fn from(k: &KeyPair) -> PublicKeyPair {
        PublicKeyPair {
            view: PublicKey::from_private_key(&k.view),
            spend: PublicKey::from_private_key(&k.spend),
        }
    }
}
