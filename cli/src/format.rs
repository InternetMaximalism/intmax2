use intmax2_interfaces::{
    data::deposit_data::TokenType,
    utils::{
        key::{KeyPair, PrivateKey},
        key_derivation::derive_keypair_from_spend_key,
    },
};
use intmax2_zkp::ethereum_types::{
    address::Address, bytes32::Bytes32, u256::U256, u32limb_trait::U32LimbTrait as _,
};

#[derive(Debug, thiserror::Error)]
pub enum FormatTokenInfoError {
    #[error("Missing amount")]
    MissingAmount,
    #[error("Missing token address")]
    MissingTokenAddress,
    #[error("Missing token id")]
    MissingTokenId,
    #[error("Amount should not be specified")]
    AmountShouldNotBeSpecified,
}

pub struct TokenInput {
    pub token_type: TokenType,
    pub amount: Option<U256>,
    pub token_address: Option<Address>,
    pub token_id: Option<U256>,
}

pub fn format_token_info(input: TokenInput) -> Result<(U256, Address, U256), FormatTokenInfoError> {
    match input.token_type {
        TokenType::NATIVE => Ok((
            input.amount.ok_or(FormatTokenInfoError::MissingAmount)?,
            Address::zero(),
            U256::zero(),
        )),
        TokenType::ERC20 => Ok((
            input.amount.ok_or(FormatTokenInfoError::MissingAmount)?,
            input
                .token_address
                .ok_or(FormatTokenInfoError::MissingTokenAddress)?,
            U256::zero(),
        )),
        TokenType::ERC721 => {
            if input.amount.is_some() {
                return Err(FormatTokenInfoError::AmountShouldNotBeSpecified);
            }
            Ok((
                U256::one(),
                input
                    .token_address
                    .ok_or(FormatTokenInfoError::MissingTokenAddress)?,
                input.token_id.ok_or(FormatTokenInfoError::MissingTokenId)?,
            ))
        }
        TokenType::ERC1155 => Ok((
            input.amount.ok_or(FormatTokenInfoError::MissingAmount)?,
            input
                .token_address
                .ok_or(FormatTokenInfoError::MissingTokenAddress)?,
            input.token_id.ok_or(FormatTokenInfoError::MissingTokenId)?,
        )),
    }
}

pub fn privkey_to_keypair(privkey: Bytes32) -> KeyPair {
    derive_keypair_from_spend_key(PrivateKey(privkey.into()), false)
}
