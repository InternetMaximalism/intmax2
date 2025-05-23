use alloy::primitives::{Address, B256};
use intmax2_client_sdk::external_api::{
    contract::{
        convert::convert_address_to_intmax,
        rollup_contract::RollupContract,
        utils::{get_address_from_private_key, get_provider_with_fallback},
    },
    utils::time::sleep_for,
};
use intmax2_zkp::{
    common::{
        block_builder::{construct_signature, SenderWithSignature},
        signature_content::{
            block_sign_payload::BlockSignPayload, key_set::KeySet, utils::get_pubkey_hash,
        },
    },
    constants::NUM_SENDERS_IN_BLOCK,
    ethereum_types::{bytes32::Bytes32, u32limb_trait::U32LimbTrait},
};
use num_bigint::BigUint;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct EnvVar {
    // client settings
    pub deployer_private_key: B256,
    pub l2_rpc_url: String,
    pub rollup_contract_address: Address,
    pub rollup_contract_deployed_block_number: u64,
}

#[tokio::test]
#[ignore]
async fn post_blocks() -> anyhow::Result<()> {
    let mut rng = intmax2_interfaces::utils::random::default_rng();
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .init();

    dotenvy::dotenv().ok();
    let env = envy::from_env::<EnvVar>()?;

    let provider = get_provider_with_fallback(std::slice::from_ref(&env.l2_rpc_url))?;
    let rollup_contract = RollupContract::new(provider, env.rollup_contract_address);
    let block_builder_address =
        convert_address_to_intmax(get_address_from_private_key(env.deployer_private_key));

    loop {
        let payload = BlockSignPayload {
            is_registration_block: true,
            tx_tree_root: Bytes32::rand(&mut rng),
            expiry: 0.into(),
            block_builder_address,
            block_builder_nonce: 0,
        };
        let keys = (0..NUM_SENDERS_IN_BLOCK)
            .map(|_| KeySet::rand(&mut rng))
            .collect::<Vec<_>>();
        let mut pubkeys = keys.iter().map(|key| key.pubkey).collect::<Vec<_>>();
        pubkeys.sort_by(|a, b| b.cmp(a));
        let pubkey_hash = get_pubkey_hash(&pubkeys);
        let signatures = keys
            .iter()
            .map(|key| SenderWithSignature {
                sender: key.pubkey,
                signature: Some(payload.sign(key.privkey, pubkey_hash)),
            })
            .collect::<Vec<_>>();
        let signature = construct_signature(&payload, pubkey_hash, Bytes32::default(), &signatures);
        rollup_contract
            .post_registration_block(
                env.deployer_private_key,
                None,
                BigUint::from(10u32).pow(17).try_into().unwrap(),
                signature.block_sign_payload.tx_tree_root,
                signature.block_sign_payload.expiry.into(),
                0,
                signature.sender_flag,
                signature.agg_pubkey,
                signature.agg_signature,
                signature.message_point,
                pubkeys,
            )
            .await?;
        println!("Posted a block ✅");
        sleep_for(7).await;
    }
}
