use ethers::types::{Address, H256};
use intmax2_client_sdk::external_api::contract::{
    erc20_contract::ERC20Contract, liquidity_contract::LiquidityContract,
    rollup_contract::RollupContract,
};
use serde::Deserialize;

#[derive(Deserialize)]
struct Config {
    pub rpc_url: String,
    pub chain_id: u64,
    pub deployer_private_key: H256,
    pub token_holder: Address,
}

#[tokio::test]
async fn deploy_contracts() -> anyhow::Result<()> {
    dotenv::dotenv().ok();
    let config = envy::from_env::<Config>().unwrap();

    let rollup_contract = RollupContract::deploy(
        &config.rpc_url,
        config.chain_id,
        config.deployer_private_key,
    )
    .await?;
    let random_address = ethers::types::Address::random();
    rollup_contract
        .initialize(
            config.deployer_private_key,
            random_address,
            random_address,
            random_address,
            random_address,
        )
        .await?;

    println!("Rollup contract address: {:?}", rollup_contract.address());
    println!(
        "Rollup contract deployed block number: {}",
        rollup_contract.deployed_block_number
    );

    let liquidity_contract = LiquidityContract::deploy(
        &config.rpc_url,
        config.chain_id,
        config.deployer_private_key,
    )
    .await?;
    liquidity_contract
        .initialize(
            config.deployer_private_key,
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
        liquidity_contract.address()
    );

    let erc20_token = ERC20Contract::deploy(
        &config.rpc_url,
        config.chain_id,
        config.deployer_private_key,
        config.token_holder,
    )
    .await?;
    println!("erc20 contract address: {:?}", erc20_token.address());

    Ok(())
}
