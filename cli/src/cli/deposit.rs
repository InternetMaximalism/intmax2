use alloy::providers::Provider;
use intmax2_client_sdk::{
    client::client::Client,
    external_api::{
        contract::{
            convert::{
                convert_address_to_alloy, convert_address_to_intmax, convert_bytes32_to_b256,
                convert_u256_to_alloy,
            },
            erc1155_contract::ERC1155Contract,
            erc20_contract::ERC20Contract,
            erc721_contract::ERC721Contract,
            liquidity_contract::LiquidityContract,
            utils::get_address_from_private_key,
        },
        predicate::{PermissionRequest, PredicateClient},
    },
};
use intmax2_interfaces::data::deposit_data::TokenType;
use intmax2_zkp::{
    common::signature_content::key_set::KeySet,
    ethereum_types::{address::Address, bytes32::Bytes32, u256::U256},
};

use crate::env_var::EnvVar;

use super::{client::get_client, error::CliError, utils::is_local};

pub async fn deposit(
    key: KeySet,
    eth_private_key: Bytes32,
    token_type: TokenType,
    amount: U256,
    token_address: Address,
    token_id: U256,
    is_mining: bool,
) -> Result<(), CliError> {
    let client = get_client()?;
    let liquidity_contract = client.liquidity_contract.clone();
    balance_check_and_approve(
        &liquidity_contract,
        eth_private_key,
        amount,
        token_type,
        token_address,
        token_id,
    )
    .await?;

    log::info!("Balance check done");

    let signer_private_key = convert_bytes32_to_b256(eth_private_key);
    let depositor = get_address_from_private_key(signer_private_key);
    let depositor = convert_address_to_intmax(depositor);

    let deposit_result = client
        .prepare_deposit(
            depositor,
            key.pubkey,
            amount,
            token_type,
            token_address,
            token_id,
            is_mining,
        )
        .await?;
    let deposit_data = deposit_result.deposit_data;

    let aml_permission = fetch_predicate_permission(
        &client,
        depositor,
        deposit_data.pubkey_salt_hash,
        token_type,
        amount,
        token_address,
        token_id,
    )
    .await?;
    let eligibility_permission = vec![];

    match token_type {
        TokenType::NATIVE => {
            liquidity_contract
                .deposit_native(
                    signer_private_key,
                    None,
                    deposit_data.pubkey_salt_hash,
                    deposit_data.amount,
                    &aml_permission,
                    &eligibility_permission,
                )
                .await?;
        }
        TokenType::ERC20 => {
            liquidity_contract
                .deposit_erc20(
                    signer_private_key,
                    None,
                    deposit_data.pubkey_salt_hash,
                    deposit_data.amount,
                    deposit_data.token_address,
                    &aml_permission,
                    &eligibility_permission,
                )
                .await?;
        }
        TokenType::ERC721 => {
            liquidity_contract
                .deposit_erc721(
                    signer_private_key,
                    None,
                    deposit_data.pubkey_salt_hash,
                    deposit_data.token_address,
                    deposit_data.token_id,
                    &aml_permission,
                    &eligibility_permission,
                )
                .await?;
        }
        TokenType::ERC1155 => {
            liquidity_contract
                .deposit_erc1155(
                    signer_private_key,
                    None,
                    deposit_data.pubkey_salt_hash,
                    deposit_data.token_address,
                    deposit_data.token_id,
                    deposit_data.amount,
                    &aml_permission,
                    &eligibility_permission,
                )
                .await?;
        }
    }

    // relay deposits by self if local
    if is_local()? {
        log::info!("get token index");
        let token_index = liquidity_contract
            .get_token_index(token_type, token_address, token_id)
            .await?
            .ok_or(CliError::UnexpectedError(
                "Cloud not find token index".to_string(),
            ))?;
        log::info!("token index: {}", token_index);
        let mut deposit_data = deposit_data;
        deposit_data.set_token_index(token_index);
        client
            .rollup_contract
            .process_deposits(
                signer_private_key,
                None,
                0,
                &[deposit_data.deposit_hash().unwrap()],
            )
            .await?;
    }

    Ok(())
}

async fn balance_check_and_approve(
    liquidity_contract: &LiquidityContract,
    eth_private_key: Bytes32,
    amount: U256,
    token_type: TokenType,
    token_address: Address,
    token_id: U256,
) -> Result<(), CliError> {
    let sender_private_key = convert_bytes32_to_b256(eth_private_key);
    let sender_address = get_address_from_private_key(sender_private_key);

    let amount = convert_u256_to_alloy(amount);
    let token_address = convert_address_to_alloy(token_address);
    let token_id = convert_u256_to_alloy(token_id);

    let provider = liquidity_contract.provider.clone();

    match token_type {
        TokenType::NATIVE => {
            let balance = provider.get_balance(sender_address).await?;
            if amount > balance {
                return Err(CliError::InsufficientBalance(
                    "Insufficient eth balance".to_string(),
                ));
            }
        }
        TokenType::ERC20 => {
            let contract = ERC20Contract::new(provider, token_address);
            let balance = contract.balance_of(sender_address).await?;
            if amount > balance {
                return Err(CliError::InsufficientBalance(
                    "Insufficient token balance".to_string(),
                ));
            }
            // approve if necessary
            let allowance = contract
                .allowance(sender_address, liquidity_contract.address)
                .await?;
            if allowance < amount {
                contract
                    .approve(sender_private_key, None, liquidity_contract.address, amount)
                    .await?;
            }
        }
        TokenType::ERC721 => {
            let contract = ERC721Contract::new(provider, token_address);
            let owner = contract.owner_of(token_id).await?;
            if owner != sender_address {
                return Err(CliError::InsufficientBalance(
                    "You don't have the nft of given token id".to_string(),
                ));
            }
            // approve if necessary
            let operator = contract.get_approved(token_id).await?;
            if operator != liquidity_contract.address {
                contract
                    .approve(
                        sender_private_key,
                        None,
                        liquidity_contract.address,
                        token_id,
                    )
                    .await?;
            }
        }
        TokenType::ERC1155 => {
            let contract = ERC1155Contract::new(provider, token_address);
            let balance = contract.balance_of(sender_address, token_id).await?;
            if amount > balance {
                return Err(CliError::InsufficientBalance(
                    "Insufficient token balance".to_string(),
                ));
            }
            // approve if necessary
            let is_approved = contract
                .is_approved_for_all(sender_address, liquidity_contract.address)
                .await?;

            if !is_approved {
                contract
                    .set_approval_for_all(
                        sender_private_key,
                        None,
                        liquidity_contract.address,
                        true,
                    )
                    .await?;
            }
        }
    }

    Ok(())
}

pub async fn fetch_predicate_permission(
    client: &Client,
    from: Address,
    recipient_salt_hash: Bytes32,
    token_type: TokenType,
    amount: U256,
    token_address: Address,
    token_id: U256,
) -> Result<Vec<u8>, CliError> {
    let aml_permitter_address = client.liquidity_contract.get_aml_permitter().await?;
    let env = envy::from_env::<EnvVar>()?;
    if aml_permitter_address.is_zero() {
        log::info!("AML predicate is not set");
        return Ok(vec![]);
    }
    if env.predicate_base_url.is_none() {
        return Err(CliError::EnvError(
            "Predicate base url must be set".to_string(),
        ));
    }
    let predicate_client = PredicateClient::new(env.predicate_base_url.unwrap());
    let recipient_salt_hash = convert_bytes32_to_b256(recipient_salt_hash);
    let token_address = convert_address_to_alloy(token_address);
    let value = if token_type == TokenType::NATIVE {
        amount
    } else {
        0.into()
    };
    let value = convert_u256_to_alloy(value);
    let amount = convert_u256_to_alloy(amount);
    let token_id = convert_u256_to_alloy(token_id);
    let request = match token_type {
        TokenType::NATIVE => PermissionRequest::Native {
            recipient_salt_hash,
            amount,
        },
        TokenType::ERC20 => PermissionRequest::ERC20 {
            recipient_salt_hash,
            token_address,
            amount,
        },
        TokenType::ERC721 => PermissionRequest::ERC721 {
            recipient_salt_hash,
            token_address,
            token_id,
        },
        TokenType::ERC1155 => PermissionRequest::ERC1155 {
            recipient_salt_hash,
            token_address,
            token_id,
            amount,
        },
    };
    let from = convert_address_to_alloy(from);
    let permission = predicate_client
        .get_deposit_permission(from, aml_permitter_address, value, request)
        .await?;
    Ok(permission)
}
