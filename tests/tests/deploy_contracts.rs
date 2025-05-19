use alloy::primitives::{Address, B256, U256};
use intmax2_client_sdk::external_api::contract::{
    block_builder_registry::BlockBuilderRegistryContract,
    erc1155_contract::ERC1155Contract,
    erc20_contract::ERC20Contract,
    erc721_contract::ERC721Contract,
    liquidity_contract::LiquidityContract,
    rollup_contract::RollupContract,
    utils::{get_address_from_private_key, get_provider_with_fallback},
    withdrawal_contract::WithdrawalContract,
};
use serde::Deserialize;

#[derive(Deserialize)]
struct EnvVar {
    pub rpc_url: String,
    pub deployer_private_key: B256,
}

#[tokio::test]
#[ignore]
async fn deploy_contracts() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    let config = envy::from_env::<EnvVar>().unwrap();

    let provider = get_provider_with_fallback(&[config.rpc_url.clone()]).unwrap();

    let rollup_contract =
        RollupContract::deploy(provider.clone(), config.deployer_private_key).await?;
    let random_address = Address::random();
    rollup_contract
        .initialize(
            config.deployer_private_key,
            random_address,
            random_address,
            random_address,
            random_address,
        )
        .await?;

    println!("Rollup contract address: {:?}", rollup_contract.address);

    let liquidity_contract =
        LiquidityContract::deploy(provider.clone(), config.deployer_private_key).await?;
    liquidity_contract
        .initialize(
            config.deployer_private_key,
            random_address,
            random_address,
            random_address,
            random_address,
            random_address,
            random_address,
            random_address,
            vec![],
        )
        .await?;

    println!(
        "Liquidity contract address: {:?}",
        liquidity_contract.address
    );

    let registry_contract =
        BlockBuilderRegistryContract::deploy(provider.clone(), config.deployer_private_key).await?;

    println!("registry contract address: {:?}", registry_contract.address);

    let withdrawal_contract =
        WithdrawalContract::deploy(provider.clone(), config.deployer_private_key).await?;
    withdrawal_contract
        .initialize(
            config.deployer_private_key,
            random_address,
            random_address,
            random_address,
            random_address,
            random_address,
            random_address,
            vec![U256::from(0), U256::from(1), U256::from(2)],
        )
        .await?;
    println!(
        "withdrawal contract address: {:?}",
        withdrawal_contract.address
    );

    let deployer = get_address_from_private_key(config.deployer_private_key);
    let erc20_token =
        ERC20Contract::deploy(provider.clone(), config.deployer_private_key, deployer).await?;
    println!("erc20 contract address: {:?}", erc20_token.address);

    let erc721_token =
        ERC721Contract::deploy(provider.clone(), config.deployer_private_key).await?;
    println!("erc721 contract address: {:?}", erc721_token.address);

    let erc1155_token =
        ERC1155Contract::deploy(provider.clone(), config.deployer_private_key).await?;
    // mint some token
    erc1155_token.setup(config.deployer_private_key).await?;

    println!("erc1155 contract address: {:?}", erc1155_token.address);

    Ok(())
}
