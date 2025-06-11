use core::fmt;
use std::str::FromStr;

use ark_bn254::{Fq, G1Affine};
use intmax2_zkp::{
    common::signature_content::key_set::KeySet,
    ethereum_types::{u256::U256, u32limb_trait::U32LimbTrait},
};
use num_bigint::BigUint;
use plonky2_bn254::fields::{recover::RecoverFromX, sgn::Sgn as _};
use rand::Rng;
use serde_with::{DeserializeFromStr, SerializeDisplay};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Invalid length")]
    InvalidLength,

    #[error("Invalid prefix for key: {0}")]
    InvalidPrefix(String),

    #[error("Invalid public key")]
    InvalidPublicKey,

    #[error(transparent)]
    EthereumTypeError(#[from] intmax2_zkp::ethereum_types::EthereumTypeError),
}

#[derive(Debug, PartialEq, Eq, Copy, Clone, Hash, SerializeDisplay, DeserializeFromStr)]
pub struct PublicKey(pub U256);

impl Default for PublicKey {
    fn default() -> Self {
        // Use a dummy public key for default
        PublicKey(U256::dummy_pubkey())
    }
}

impl PublicKey {
    pub fn to_bytes(self) -> [u8; 32] {
        self.0.to_bytes_be().try_into().unwrap()
    }

    pub fn from_slice(data: &[u8]) -> Result<PublicKey, Error> {
        if data.len() != 32 {
            return Err(Error::InvalidLength);
        }
        let pubkey = PublicKey(U256::from_bytes_be(data)?);
        if !pubkey.is_valid() {
            return Err(Error::InvalidPublicKey);
        }
        Ok(pubkey)
    }

    pub fn from_private_key(privkey: &PrivateKey) -> PublicKey {
        let key = KeySet::new(privkey.0);
        PublicKey(key.pubkey)
    }

    pub fn is_valid(&self) -> bool {
        let fq_max = BigUint::from(Fq::from(-1));
        if BigUint::from(self.0) > fq_max {
            return false;
        }
        let x = Fq::from(self.0);
        let is_recoverable = G1Affine::is_recoverable_from_x(x);
        if !is_recoverable {
            return false;
        }
        let recovered_pubkey = G1Affine::recover_from_x(x);
        let y_sgn = recovered_pubkey.y.sgn();
        if y_sgn {
            // If y is odd, the public key is invalid
            return false;
        }
        true
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
        let pubkey = PublicKey(U256::from_hex(s)?);
        if !pubkey.is_valid() {
            return Err(Error::InvalidPublicKey);
        }
        Ok(pubkey)
    }
}

impl TryFrom<&[u8]> for PublicKey {
    type Error = Error;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        Self::from_slice(value)
    }
}

#[derive(Debug, PartialEq, Eq, Copy, Clone, Hash, SerializeDisplay, DeserializeFromStr)]
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
        let key = KeySet::rand(rng);
        PrivateKey(key.privkey)
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

#[derive(Debug, PartialEq, Eq, Copy, Clone, Hash, SerializeDisplay, DeserializeFromStr)]
pub struct KeyPair {
    pub view: PrivateKey,
    pub spend: PrivateKey,
}

impl fmt::Display for KeyPair {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "keypair/{}/{}", self.view, self.spend)
    }
}

impl FromStr for KeyPair {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split('/').collect();
        if parts.len() != 3 {
            return Err(Error::InvalidLength);
        }
        if parts[0] != "keypair" {
            return Err(Error::InvalidPrefix(parts[0].to_string()));
        }
        let view = PrivateKey::from_str(parts[1])?;
        let spend = PrivateKey::from_str(parts[2])?;
        Ok(KeyPair { view, spend })
    }
}

#[derive(Debug, PartialEq, Eq, Copy, Clone, Hash, SerializeDisplay, DeserializeFromStr)]
pub struct ViewPair {
    pub view: PrivateKey,
    pub spend: PublicKey,
}

impl fmt::Display for ViewPair {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "viewpair/{}/{}", self.view, self.spend)
    }
}

impl FromStr for ViewPair {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split('/').collect();
        if parts.len() != 3 {
            return Err(Error::InvalidLength);
        }
        if parts[0] != "viewpair" {
            return Err(Error::InvalidPrefix(parts[0].to_string()));
        }
        let view = PrivateKey::from_str(parts[1])?;
        let spend = PublicKey::from_str(parts[2])?;
        Ok(ViewPair { view, spend })
    }
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

#[derive(
    Default, Debug, PartialEq, Eq, Copy, Clone, Hash, SerializeDisplay, DeserializeFromStr,
)]
pub struct PublicKeyPair {
    pub view: PublicKey,
    pub spend: PublicKey,
}

impl fmt::Display for PublicKeyPair {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "pubkeypair/{}/{}", self.view, self.spend)
    }
}

impl FromStr for PublicKeyPair {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split('/').collect();
        if parts.len() != 3 {
            return Err(Error::InvalidLength);
        }
        if parts[0] != "pubkeypair" {
            return Err(Error::InvalidPrefix(parts[0].to_string()));
        }
        let view = PublicKey::from_str(parts[1])?;
        let spend = PublicKey::from_str(parts[2])?;
        Ok(PublicKeyPair { view, spend })
    }
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
