use ethers::types::H256;
use intmax2_core_sdk::{
    client::{client::Client, config::ClientConfig},
    external_api::{
        balance_prover::test_server::server::TestBalanceProver,
        block_builder::test_server::server::TestBlockBuilder,
        block_validity_prover::test_server::server::TestBlockValidityProver,
        contract::{interface::ContractInterface, test_server::server::TestContract},
        store_vault_server::test_server::server::TestStoreVaultServer,
        withdrawal_aggregator::test_server::server::TestWithdrawalAggregator,
    },
};
use intmax2_zkp::{
    common::{
        generic_address::GenericAddress, salt::Salt, signature::key_set::KeySet, transfer::Transfer,
    },
    ethereum_types::u256::U256,
};
use num_bigint::BigUint;

type BC = TestContract;
type BB = TestBlockBuilder;
type S = TestStoreVaultServer;
type V = TestBlockValidityProver;
type B = TestBalanceProver;
type W = TestWithdrawalAggregator;

const BASE_URL: &str = "http://localhost:9563";

pub fn get_client() -> anyhow::Result<Client<BB, S, V, B, W>> {
    let base_url = BASE_URL.to_string();
    let block_builder = BB::new(base_url.clone());
    let store_vault_server = S::new(base_url.clone());
    let validity_prover = V::new(base_url.clone());
    let balance_prover = B::new(base_url.clone());
    let withdrawal_aggregator = W::new(base_url.clone());

    let config = ClientConfig {
        deposit_timeout: 3600,
        tx_timeout: 60,
        max_tx_query_times: 50,
        tx_query_interval: 1,
    };

    let client = Client {
        block_builder,
        store_vault_server,
        validity_prover,
        balance_prover,
        withdrawal_aggregator,
        config,
    };

    Ok(client)
}

pub fn get_contract() -> BC {
    let base_url = BASE_URL.to_string();
    let contract = BC::new(base_url.clone());
    contract
}

pub async fn deposit(
    _rpc_url: &str,
    eth_private_key: H256,
    private_key: H256,
    amount: U256,
    token_index: u32,
) -> anyhow::Result<()> {
    let client = get_client()?;
    let key = h256_to_keyset(private_key);
    let deposit_call = client.prepare_deposit(key, token_index, amount).await?;

    let contract = get_contract();
    contract
        .deposit_native_token(
            eth_private_key,
            deposit_call.pubkey_salt_hash,
            deposit_call.amount,
        )
        .await?;
    Ok(())
}

pub async fn tx(
    block_builder_url: &str,
    private_key: H256,
    to: U256,
    amount: U256,
    token_index: u32,
) -> anyhow::Result<()> {
    let client = get_client()?;
    let key = h256_to_keyset(private_key);

    let mut rng = rand::thread_rng();
    let salt = Salt::rand(&mut rng);
    let transfer = Transfer {
        recipient: GenericAddress::from_pubkey(to),
        amount,
        token_index,
        salt,
    };
    let memo = client
        .send_tx_request(block_builder_url, key, vec![transfer])
        .await?;

    // sleep for a while to wait for the block builder to build the block
    tokio::time::sleep(std::time::Duration::from_secs(30)).await;

    log::info!("Finalizing tx");
    client.finalize_tx(block_builder_url, key, &memo).await?;

    Ok(())
}

pub async fn sync(private_key: H256) -> anyhow::Result<()> {
    let client = get_client()?;
    let key = h256_to_keyset(private_key);
    client.sync(key).await?;
    Ok(())
}

pub async fn balance(private_key: H256) -> anyhow::Result<()> {
    let client = get_client()?;
    let key = h256_to_keyset(private_key);
    client.sync(key).await?;

    let user_data = client.get_user_data(key).await?;
    let balances = user_data.balances();
    for (i, leaf) in balances.iter() {
        println!("Token {}: {}", i, leaf.amount);
    }
    println!("end");
    Ok(())
}

fn h256_to_keyset(h256: H256) -> KeySet {
    KeySet::new(BigUint::from_bytes_be(h256.as_bytes()).into())
}
