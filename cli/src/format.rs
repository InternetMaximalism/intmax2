use anyhow::{bail, ensure};
use ethers::types::{Address as EthAddress, H256};
use intmax2_interfaces::data::deposit_data::TokenType;
use intmax2_zkp::{
    common::{generic_address::GenericAddress, signature::key_set::KeySet},
    ethereum_types::{
        address::Address as IAddress, u256::U256 as IU256, u32limb_trait::U32LimbTrait as _,
    },
};
use num_bigint::BigUint;

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
    amount: Option<IU256>,
    token_address: Option<EthAddress>,
    token_id: Option<IU256>,
) -> Result<(IU256, EthAddress, IU256), FormatTokenInfoError> {
    match token_type {
        TokenType::NATIVE => Ok({
            let amount = amount.ok_or(FormatTokenInfoError::MissingAmount)?;
            (amount, EthAddress::zero(), IU256::zero())
        }),
        TokenType::ERC20 => {
            let amount = amount.ok_or(FormatTokenInfoError::MissingAmount)?;
            let token_address = token_address.ok_or(FormatTokenInfoError::MissingTokenAddress)?;
            Ok((amount, token_address, IU256::zero()))
        }
        TokenType::ERC721 => {
            if amount.is_some() {
                return Err(FormatTokenInfoError::AmountShouldNotBeSpecified);
            }
            let token_address = token_address.ok_or(FormatTokenInfoError::MissingTokenAddress)?;
            let token_id = token_id.ok_or(FormatTokenInfoError::MissingTokenId)?;
            Ok((IU256::one(), token_address, token_id))
        }
        TokenType::ERC1155 => {
            let amount = amount.ok_or(FormatTokenInfoError::MissingAmount)?;
            let token_address = token_address.ok_or(FormatTokenInfoError::MissingTokenAddress)?;
            let token_id = token_id.ok_or(FormatTokenInfoError::MissingTokenId)?;
            Ok((amount, token_address, token_id))
        }
    }
}

pub fn privkey_to_keyset(privkey: H256) -> KeySet {
    KeySet::new(BigUint::from_bytes_be(privkey.as_bytes()).into())
}

pub fn parse_generic_address(address: &str) -> anyhow::Result<GenericAddress> {
    ensure!(address.starts_with("0x"), "Invalid prefix");
    let bytes = hex::decode(&address[2..])?;
    if bytes.len() == 20 {
        let address = IAddress::from_bytes_be(&bytes);
        Ok(GenericAddress::from_address(address))
    } else if bytes.len() == 32 {
        let pubkey = IU256::from_bytes_be(&bytes);
        Ok(GenericAddress::from_pubkey(pubkey))
    } else {
        bail!("Invalid length");
    }
}
