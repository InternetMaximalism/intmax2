use ethers::types::{Address, H256};
use intmax2_client_sdk::external_api::contract::{
    erc1155_contract::ERC1155Contract,
    erc20_contract::ERC20Contract,
    erc721_contract::ERC721Contract,
    liquidity_contract::LiquidityContract,
    utils::{get_address, get_eth_balance},
};
use intmax2_interfaces::data::deposit_data::TokenType;
use intmax2_zkp::{common::signature::key_set::KeySet, ethereum_types::u256::U256};

use super::{
    client::get_client,
    error::CliError,
    utils::{convert_address, convert_u256, is_local},
};

pub async fn deposit(
    key: KeySet,
    eth_private_key: H256,
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

    let token_address = convert_address(token_address);
    let depositor = convert_address(get_address(liquidity_contract.chain_id, eth_private_key));

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

    let aml_permission = vec![];
    let eligibility_permission = vec![];

    match token_type {
        TokenType::NATIVE => {
            liquidity_contract
                .deposit_native(
                    eth_private_key,
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
                    eth_private_key,
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
                    eth_private_key,
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
                    eth_private_key,
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
            .process_deposits(eth_private_key, 0, &[deposit_data.deposit_hash().unwrap()])
            .await?;
    }

    Ok(())
}

async fn balance_check_and_approve(
    liquidity_contract: &LiquidityContract,
    eth_private_key: H256,
    amount: U256,
    token_type: TokenType,
    token_address: Address,
    token_id: U256,
) -> Result<(), CliError> {
    let amount = convert_u256(amount);
    let token_id = convert_u256(token_id);

    let chain_id = liquidity_contract.chain_id;
    let rpc_url = liquidity_contract.rpc_url.clone();
    let address = get_address(chain_id, eth_private_key);

    match token_type {
        TokenType::NATIVE => {
            let balance = get_eth_balance(&rpc_url, address).await?;
            if amount > balance {
                return Err(CliError::InsufficientBalance(
                    "Insufficient eth balance".to_string(),
                ));
            }
        }
        TokenType::ERC20 => {
            let contract = ERC20Contract::new(&rpc_url, chain_id, token_address);
            let balance = contract.balance_of(address).await?;
            if amount > balance {
                return Err(CliError::InsufficientBalance(
                    "Insufficient token balance".to_string(),
                ));
            }

            // approve if necessary
            let allowance = contract
                .allowance(address, liquidity_contract.address())
                .await?;
            if allowance < amount {
                contract
                    .approve(eth_private_key, liquidity_contract.address(), amount)
                    .await?;
            }
        }
        TokenType::ERC721 => {
            let contract = ERC721Contract::new(&rpc_url, chain_id, token_address);
            let owner = contract.owner_of(token_id).await?;
            if owner != address {
                return Err(CliError::InsufficientBalance(
                    "You don't have the nft of given token id".to_string(),
                ));
            }

            // approve if necessary
            let operator = contract.get_approved(token_id).await?;
            if operator != liquidity_contract.address() {
                contract
                    .approve(eth_private_key, liquidity_contract.address(), token_id)
                    .await?;
            }
        }
        TokenType::ERC1155 => {
            let contract = ERC1155Contract::new(&rpc_url, chain_id, token_address);
            let balance = contract.balance_of(address, token_id).await?;
            if amount > balance {
                return Err(CliError::InsufficientBalance(
                    "Insufficient token balance".to_string(),
                ));
            }
            // approve if necessary
            let is_approved = contract
                .is_approved_for_all(address, liquidity_contract.address())
                .await?;

            if !is_approved {
                contract
                    .set_approval_for_all(eth_private_key, liquidity_contract.address(), true)
                    .await?;
            }
        }
    }

    Ok(())
}
