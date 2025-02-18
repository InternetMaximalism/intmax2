use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

use intmax2_zkp::{
    common::{
        deposit::{get_pubkey_salt_hash, Deposit},
        salt::Salt,
    },
    ethereum_types::{address::Address, bytes32::Bytes32, u256::U256},
    utils::leafable::Leafable,
};

use super::{encryption::Encryption, validation::Validation};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DepositData {
    pub deposit_salt: Salt,
    pub depositor: Address,        // The address of the depositor
    pub pubkey_salt_hash: Bytes32, // The poseidon hash of the pubkey and salt, to hide the pubkey
    pub amount: U256,              // The amount of the token, which is the amount of the deposit
    pub is_eligible: bool,         // Whether the depositor is eligible to mining rewards

    // token info
    pub token_type: TokenType,
    pub token_address: Address,
    pub token_id: U256,

    // mining info
    pub is_mining: bool, // Whether the depositor is for mining

    pub token_index: Option<u32>, // The index of the token in the contract
}

#[derive(Clone, Debug, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TokenType {
    NATIVE = 0,
    ERC20 = 1,
    ERC721 = 2,
    ERC1155 = 3,
}

impl FromStr for TokenType {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, String> {
        match s {
            "NATIVE" => Ok(Self::NATIVE),
            "ERC20" => Ok(Self::ERC20),
            "ERC721" => Ok(Self::ERC721),
            "ERC1155" => Ok(Self::ERC1155),
            _ => Err("invalid token type".to_string()),
        }
    }
}

impl fmt::Display for TokenType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let t = match self {
            Self::NATIVE => "NATIVE".to_string(),
            Self::ERC20 => "ERC20".to_string(),
            Self::ERC721 => "ERC721".to_string(),
            Self::ERC1155 => "ERC1155".to_string(),
        };
        write!(f, "{}", t)
    }
}

impl TryFrom<u8> for TokenType {
    type Error = String;

    fn try_from(value: u8) -> std::result::Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::NATIVE),
            1 => Ok(Self::ERC20),
            2 => Ok(Self::ERC721),
            3 => Ok(Self::ERC1155),
            _ => Err("invalid token type".to_string()),
        }
    }
}

impl DepositData {
    pub fn set_token_index(&mut self, token_index: u32) {
        self.token_index = Some(token_index);
    }

    pub fn deposit(&self) -> Option<Deposit> {
        self.token_index.map(|token_index| Deposit {
            depositor: self.depositor,
            pubkey_salt_hash: self.pubkey_salt_hash,
            amount: self.amount,
            token_index,
            is_eligible: self.is_eligible,
        })
    }

    pub fn deposit_hash(&self) -> Option<Bytes32> {
        self.deposit().map(|deposit| deposit.hash())
    }
}

impl Encryption for DepositData {}

impl Validation for DepositData {
    fn validate(&self, pubkey: U256) -> anyhow::Result<()> {
        if self.pubkey_salt_hash != get_pubkey_salt_hash(pubkey, self.deposit_salt) {
            anyhow::bail!("Invalid pubkey_salt_hash");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::TokenType;
    use std::str::FromStr;

    #[test]
    fn test_token_type() {
        let native = TokenType::from_str("NATIVE").unwrap();
        assert_eq!(native.to_string(), "NATIVE");

        let erc721 = TokenType::ERC721;
        assert_eq!(erc721.to_string(), "ERC721");
    }
}
