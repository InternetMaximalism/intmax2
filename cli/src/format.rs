use anyhow::{bail, ensure};
use intmax2_interfaces::data::deposit_data::TokenType;
use intmax2_zkp::{
    common::{generic_address::GenericAddress, signature_content::key_set::KeySet},
    ethereum_types::{
        address::Address, bytes32::Bytes32, u256::U256, u32limb_trait::U32LimbTrait as _,
    },
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

pub fn format_token_info(
    token_type: TokenType,
    amount: Option<U256>,
    token_address: Option<Address>,
    token_id: Option<U256>,
) -> Result<(U256, Address, U256), FormatTokenInfoError> {
    match token_type {
        TokenType::NATIVE => Ok({
            let amount = amount.ok_or(FormatTokenInfoError::MissingAmount)?;
            (amount, Address::zero(), U256::zero())
        }),
        TokenType::ERC20 => {
            let amount = amount.ok_or(FormatTokenInfoError::MissingAmount)?;
            let token_address = token_address.ok_or(FormatTokenInfoError::MissingTokenAddress)?;
            Ok((amount, token_address, U256::zero()))
        }
        TokenType::ERC721 => {
            if amount.is_some() {
                return Err(FormatTokenInfoError::AmountShouldNotBeSpecified);
            }
            let token_address = token_address.ok_or(FormatTokenInfoError::MissingTokenAddress)?;
            let token_id = token_id.ok_or(FormatTokenInfoError::MissingTokenId)?;
            Ok((U256::one(), token_address, token_id))
        }
        TokenType::ERC1155 => {
            let amount = amount.ok_or(FormatTokenInfoError::MissingAmount)?;
            let token_address = token_address.ok_or(FormatTokenInfoError::MissingTokenAddress)?;
            let token_id = token_id.ok_or(FormatTokenInfoError::MissingTokenId)?;
            Ok((amount, token_address, token_id))
        }
    }
}

pub fn privkey_to_keyset(privkey: Bytes32) -> KeySet {
    KeySet::new(privkey.into())
}

pub fn parse_generic_address(address: &str) -> anyhow::Result<GenericAddress> {
    ensure!(address.starts_with("0x"), "Invalid prefix");
    let bytes = hex::decode(&address[2..])?;
    if bytes.len() == 20 {
        let address = Address::from_bytes_be(&bytes).unwrap();
        Ok(address.into())
    } else if bytes.len() == 32 {
        let pubkey = U256::from_bytes_be(&bytes).unwrap();
        Ok(pubkey.into())
    } else {
        bail!("Invalid length");
    }
}
