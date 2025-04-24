use bigdecimal::num_bigint::BigUint;
use ethers::{core::utils::Anvil, types::H256};
use intmax2_client_sdk::external_api::contract::{
    rollup_contract::RollupContract, utils::get_latest_block_number,
};
use intmax2_zkp::common::signature_content::SignatureContent;
use server_common::logger::init_logger;
use validity_prover::EnvVar;

#[tokio::test]
async fn post_blocks() -> anyhow::Result<()> {
    init_logger()?;

    let anvil = Anvil::new().spawn();
    dotenv::dotenv().ok();
    let env = envy::from_env::<EnvVar>().unwrap();

    // magic-number index=1 is key for block builder
    let block_builder_private_key: [u8; 32] = anvil.keys()[1].to_bytes().into();
    let block_builder_private_key = H256::from_slice(&block_builder_private_key);

    let mut rng = intmax2_interfaces::utils::random::default_rng();
    let rollup_contract = RollupContract::new(
        &env.l2_rpc_url,
        env.l2_chain_id,
        env.rollup_contract_address,
    );

    let block_number = get_latest_block_number(&env.l2_rpc_url).await?;
    println!("block_number: {:?}", block_number);

    let (keys, signature) = SignatureContent::rand(&mut rng);
    let pubkeys = keys.iter().map(|key| key.pubkey).collect::<Vec<_>>();
    rollup_contract
        .post_registration_block(
            block_builder_private_key,
            BigUint::from(10u32).pow(18).try_into().unwrap(),
            signature.block_sign_payload.tx_tree_root,
            signature.block_sign_payload.expiry.into(),
            signature.block_sign_payload.block_builder_nonce,
            signature.sender_flag,
            signature.agg_pubkey,
            signature.agg_signature,
            signature.message_point,
            pubkeys,
        )
        .await?;

    Ok(())
}
