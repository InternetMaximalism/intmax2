use std::{
    env,
    time::{Duration, Instant},
};

use actix_web::{web::Data, App, HttpServer};
use alloy::{
    primitives::{utils::parse_units, B256},
    providers::Provider,
    rpc::types::TransactionRequest,
    signers::local::{coins_bip39::English, MnemonicBuilder},
};
use block_builder::{
    api::{routes::block_builder_scope, state::State},
    EnvVar,
};
use intmax2_client_sdk::{
    client::config::network_from_env,
    external_api::{
        block_builder::BlockBuilderClient,
        contract::utils::{get_address_from_private_key, get_provider, get_provider_with_signer},
        validity_prover::ValidityProverClient,
    },
};
use intmax2_interfaces::{
    api::{
        block_builder::interface::BlockBuilderClientInterface,
        validity_prover::interface::ValidityProverClientInterface,
    },
    utils::{
        address::IntmaxAddress,
        key::{KeyPair, PrivateKey},
        network::Network,
    },
};
use intmax2_zkp::{common::tx::Tx, utils::poseidon_hash_out::PoseidonHashOut};
use server_common::{
    health_check::{health_check, set_name_and_version},
    logger,
};
use uuid::Uuid;

const MAX_QUERY_RETRIES: usize = 10;

async fn run_builder(env: EnvVar, port: u16, private_key: B256) {
    set_name_and_version(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
    let mut env = env;
    env.cluster_id = Some(Uuid::new_v4().to_string());
    env.block_builder_private_key = private_key;
    env.registration_fee = None;
    env.non_registration_fee = None;
    env.registration_collateral_fee = None;
    env.non_registration_collateral_fee = None;

    let state = State::new(&env).await.unwrap();
    state.run();
    let data = Data::new(state);
    HttpServer::new(move || {
        App::new()
            .app_data(data.clone())
            .service(health_check)
            .service(block_builder_scope())
    })
    .bind(format!("0.0.0.0:{port}"))
    .unwrap()
    .run()
    .await
    .unwrap();
}

async fn send_tx(
    validity_prover_client: &ValidityProverClient,
    client: &BlockBuilderClient,
    network: Network,
    block_builder_url: &str,
    sender_keys: KeyPair,
    tx: Tx,
) {
    let sender = IntmaxAddress::from_keypair(network, &sender_keys);
    let request_id = client
        .send_tx_request(block_builder_url, true, sender, tx, None)
        .await
        .unwrap();

    let mut retries = 0;
    let proposal = loop {
        if retries >= MAX_QUERY_RETRIES {
            panic!("Failed to get proposal after {MAX_QUERY_RETRIES} retries");
        }
        let proposal = client
            .query_proposal(block_builder_url, &request_id)
            .await
            .unwrap();
        match proposal {
            Some(proposal) => {
                break proposal;
            }
            None => {
                retries += 1;
                println!("Proposal not found, retrying... ({retries}/{MAX_QUERY_RETRIES})");
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            }
        }
    };

    let signature = proposal.sign(sender_keys.spend.to_key_set());
    client
        .post_signature(
            block_builder_url,
            &request_id,
            signature.pubkey,
            signature.signature,
        )
        .await
        .unwrap();

    let expiry: u64 = proposal.block_sign_payload.expiry.into();
    let deadline = Instant::now() + Duration::from_secs(expiry - current_time());
    loop {
        if Instant::now() >= deadline {
            log::error!("tx expired");
            break;
        }
        let block_number = validity_prover_client
            .get_block_number_by_tx_tree_root(proposal.block_sign_payload.tx_tree_root)
            .await
            .unwrap();
        match block_number {
            None => log::info!("tx pending"),
            Some(block_number) => {
                log::info!("tx included in block {block_number}");
                break;
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    }
}

pub fn current_time() -> u64 {
    chrono::Utc::now().timestamp() as u64
}

fn get_block_builder_url(port: u16) -> String {
    format!("http://localhost:{port}")
}

fn get_private_key(mnemonic: &str, index: u32) -> B256 {
    let signer = MnemonicBuilder::<English>::default()
        .phrase(mnemonic)
        .index(index)
        .unwrap()
        .build()
        .unwrap();
    signer.to_bytes()
}

async fn distribute(rpc_url: &str, private_key: B256, mnemonic: &str, num_builders: usize) {
    let provider = get_provider(rpc_url).unwrap();
    let signer = get_provider_with_signer(&provider, private_key);

    // send 0.1 ETH to each port
    let amount = parse_units("0.1", "ether").unwrap();
    for index in 0..num_builders {
        let private_key = get_private_key(mnemonic, index as u32);
        let address = get_address_from_private_key(private_key);

        let tx = TransactionRequest::default()
            .to(address)
            .value(amount.into());
        let _ = signer.send_transaction(tx).await.unwrap();
    }
}

#[actix_rt::test]
#[ignore]
async fn test_e2e_block_builder() {
    dotenvy::dotenv().ok();
    logger::init_logger().unwrap();

    let env = envy::from_env::<EnvVar>().unwrap();
    let network = network_from_env();

    let mnemonic = env::var("E2E_TEST_MNEMONIC").unwrap();

    let ports = (9100..9110).collect::<Vec<u16>>();

    distribute(
        &env.l2_rpc_url,
        env.block_builder_private_key,
        &mnemonic,
        ports.len(),
    )
    .await;

    for (i, &port) in ports.iter().enumerate() {
        let env = env.clone();
        let private_key = get_private_key(&mnemonic, i as u32);
        actix_rt::spawn(async move {
            run_builder(env.clone(), port, private_key).await;
        });
    }
    tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;

    let client = BlockBuilderClient::new();
    let validity_prover_client = ValidityProverClient::new(&env.validity_prover_base_url);
    let mut rng = rand::thread_rng();

    for &port in &ports {
        let block_builder_url = get_block_builder_url(port);
        let keys = KeyPair {
            view: PrivateKey::rand(&mut rng),
            spend: PrivateKey::rand(&mut rng),
        };
        let tx = Tx {
            transfer_tree_root: PoseidonHashOut::rand(&mut rng),
            nonce: 0,
        };
        let client = client.clone();
        let validity_prover_client = validity_prover_client.clone();
        actix_rt::spawn(async move {
            send_tx(
                &validity_prover_client,
                &client,
                network,
                &block_builder_url,
                keys,
                tx,
            )
            .await;
        });
    }

    tokio::time::sleep(tokio::time::Duration::from_secs(120)).await;
}
