use alloy::primitives::B256;
use intmax2_client_sdk::external_api::{
    contract::{
        convert::{convert_address_to_alloy, convert_address_to_intmax},
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
    ethereum_types::{
        account_id::{AccountId, AccountIdPacked},
        address::Address,
        bytes32::Bytes32,
        u256::U256,
        u32limb_trait::U32LimbTrait,
    },
};
use num_bigint::BigUint;
use rand::Rng;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct EnvVar {
    // client settings
    pub private_keys: String,
    pub l2_rpc_url: String,
    pub rollup_contract_address: Address,
    pub sleep_time: u64,
}

#[tokio::test]
#[ignore]
async fn load_test() -> anyhow::Result<()> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .init();
    dotenvy::dotenv().ok();

    let env = envy::from_env::<EnvVar>()?;
    let provider = get_provider_with_fallback(&[env.l2_rpc_url.clone()])?;

    let rollup_contract = RollupContract::new(
        provider,
        convert_address_to_alloy(env.rollup_contract_address),
    );

    let private_keys = env
        .private_keys
        .split(',')
        .map(|key| key.parse().unwrap())
        .collect::<Vec<B256>>();

    log::info!("num private keys: {}", private_keys.len());
    let mut total_posted_blocks = 0;

    loop {
        let mut futures: Vec<tokio::task::JoinHandle<anyhow::Result<(), anyhow::Error>>> =
            Vec::new();
        for &private_key in &private_keys {
            let block_builder_address =
                convert_address_to_intmax(get_address_from_private_key(private_key));
            let rollup_contract = rollup_contract.clone();
            futures.push(tokio::spawn(async move {
                let mut rng = intmax2_interfaces::utils::random::default_rng();
                let is_registration_block = rng.gen_bool(0.5);
                if is_registration_block {
                    post_registration_block(
                        &mut rng,
                        private_key,
                        &rollup_contract,
                        block_builder_address,
                    )
                    .await?;
                    log::info!("Posted registration block");
                } else {
                    post_non_registration_block(
                        &mut rng,
                        private_key,
                        &rollup_contract,
                        block_builder_address,
                    )
                    .await?;
                    log::info!("Posted non registration block");
                }
                Ok(())
            }));
        }
        // await all
        let results = futures::future::join_all(futures).await;
        for result in results {
            if let Err(e) = result? {
                log::error!("Error posting block: {:?}", e);
            }
        }
        total_posted_blocks += private_keys.len();
        log::info!("Posted {} blocks", total_posted_blocks);
        sleep_for(env.sleep_time).await;
    }
}

async fn post_registration_block<R: Rng>(
    rng: &mut R,
    signer_private_key: B256,
    rollup_contract: &RollupContract,
    block_builder_address: Address,
) -> anyhow::Result<()> {
    let payload = BlockSignPayload {
        is_registration_block: true,
        tx_tree_root: Bytes32::rand(rng),
        expiry: 0.into(),
        block_builder_address,
        block_builder_nonce: 0,
    };
    let num_senders = rng.gen_range(1..=NUM_SENDERS_IN_BLOCK);
    let keys = (0..num_senders)
        .map(|_| KeySet::rand(rng))
        .collect::<Vec<_>>();
    let mut pubkeys = keys.iter().map(|key| key.pubkey).collect::<Vec<_>>();
    pubkeys.resize(NUM_SENDERS_IN_BLOCK, U256::dummy_pubkey());
    pubkeys.sort_by(|a, b| b.cmp(a));
    let pubkey_hash = get_pubkey_hash(&pubkeys);
    let mut signatures = keys
        .iter()
        .map(|key| SenderWithSignature {
            sender: key.pubkey,
            signature: Some(payload.sign(key.privkey, pubkey_hash)),
        })
        .collect::<Vec<_>>();
    signatures.resize(
        NUM_SENDERS_IN_BLOCK,
        SenderWithSignature {
            sender: U256::dummy_pubkey(),
            signature: None,
        },
    );
    let signature = construct_signature(&payload, pubkey_hash, Bytes32::default(), &signatures);

    let pubkeys_trimmed = pubkeys
        .into_iter()
        .filter(|pubkey| *pubkey != U256::dummy_pubkey())
        .collect::<Vec<_>>();
    rollup_contract
        .post_registration_block(
            signer_private_key,
            Some(400000),
            BigUint::from(10u32).pow(16).try_into().unwrap(),
            signature.block_sign_payload.tx_tree_root,
            signature.block_sign_payload.expiry.into(),
            0,
            signature.sender_flag,
            signature.agg_pubkey,
            signature.agg_signature,
            signature.message_point,
            pubkeys_trimmed,
        )
        .await?;
    Ok(())
}

async fn post_non_registration_block<R: Rng>(
    rng: &mut R,
    signer_private_key: B256,
    rollup_contract: &RollupContract,
    block_builder_address: Address,
) -> anyhow::Result<()> {
    let payload = BlockSignPayload {
        is_registration_block: false,
        tx_tree_root: Bytes32::rand(rng),
        expiry: 0.into(),
        block_builder_address,
        block_builder_nonce: 0,
    };
    let num_senders = rng.gen_range(1..=NUM_SENDERS_IN_BLOCK);
    let keys = (0..num_senders)
        .map(|_| KeySet::rand(rng))
        .collect::<Vec<_>>();
    let mut pubkeys = keys.iter().map(|key| key.pubkey).collect::<Vec<_>>();
    pubkeys.resize(NUM_SENDERS_IN_BLOCK, U256::dummy_pubkey());
    pubkeys.sort_by(|a, b| b.cmp(a));
    let pubkey_hash = get_pubkey_hash(&pubkeys);

    let mut account_ids = keys
        .iter()
        .map(|_| AccountId(rng.gen_range(0..1 << 40)))
        .collect::<Vec<_>>();
    account_ids.resize(NUM_SENDERS_IN_BLOCK, AccountId::dummy());
    let account_ids = AccountIdPacked::pack(&account_ids);

    let mut signatures = keys
        .iter()
        .map(|key| SenderWithSignature {
            sender: key.pubkey,
            signature: Some(payload.sign(key.privkey, pubkey_hash)),
        })
        .collect::<Vec<_>>();
    signatures.resize(
        NUM_SENDERS_IN_BLOCK,
        SenderWithSignature {
            sender: U256::dummy_pubkey(),
            signature: None,
        },
    );

    let signature = construct_signature(&payload, pubkey_hash, Bytes32::default(), &signatures);
    rollup_contract
        .post_non_registration_block(
            signer_private_key,
            Some(400000),
            BigUint::from(10u32).pow(16).try_into().unwrap(),
            signature.block_sign_payload.tx_tree_root,
            signature.block_sign_payload.expiry.into(),
            0,
            signature.sender_flag,
            signature.agg_pubkey,
            signature.agg_signature,
            signature.message_point,
            pubkey_hash,
            account_ids.to_trimmed_bytes(),
        )
        .await?;
    Ok(())
}
