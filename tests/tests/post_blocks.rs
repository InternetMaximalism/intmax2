use ethers::types::{Address, H256};
use intmax2_client_sdk::external_api::{
    contract::rollup_contract::RollupContract, utils::time::sleep_for,
};
use intmax2_zkp::common::signature::SignatureContent;
use num_bigint::BigUint;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct EnvVar {
    // client settings
    pub deployer_private_key: H256,
    pub l2_rpc_url: String,
    pub l2_chain_id: u64,
    pub rollup_contract_address: Address,
    pub rollup_contract_deployed_block_number: u64,
}

#[tokio::test]
#[ignore]
async fn post_blocks() -> anyhow::Result<()> {
    let mut rng = rand::thread_rng();
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .init();

    dotenv::dotenv().ok();
    let env = envy::from_env::<EnvVar>()?;
    let rollup_contract = RollupContract::new(
        &env.l2_rpc_url,
        env.l2_chain_id,
        env.rollup_contract_address,
        env.rollup_contract_deployed_block_number,
    );

    loop {
        let (keys, signature) = SignatureContent::rand(&mut rng);
        let pubkeys = keys.iter().map(|key| key.pubkey).collect::<Vec<_>>();
        rollup_contract
            .post_registration_block(
                env.deployer_private_key,
                BigUint::from(10u32).pow(17).try_into().unwrap(),
                signature.tx_tree_root,
                signature.expiry.into(),
                signature.sender_flag,
                signature.agg_pubkey,
                signature.agg_signature,
                signature.message_point,
                pubkeys,
            )
            .await?;

        sleep_for(7).await;
    }
}
