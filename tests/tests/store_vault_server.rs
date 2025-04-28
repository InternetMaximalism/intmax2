use ethers::types::H256;
use intmax2_client_sdk::external_api::store_vault_server::StoreVaultServerClient;
use intmax2_interfaces::{
    api::store_vault_server::interface::StoreVaultClientInterface, data::user_data::UserData,
};
use intmax2_zkp::{common::signature::key_set::KeySet, ethereum_types::u256::U256};
use num_bigint::BigUint;
use serde::Deserialize;

#[derive(Deserialize)]
struct EnvVar {
    pub store_vault_server_base_url: String,
}

#[tokio::test]
async fn reset_user_data() -> anyhow::Result<()> {
    dotenv::dotenv().ok();
    let config = envy::from_env::<EnvVar>().unwrap();
    let pubkey_hex: H256 = std::env::var("PUBKEY").unwrap().parse().unwrap();
    let pubkey: U256 = BigUint::from_bytes_be(pubkey_hex.as_bytes())
        .try_into()
        .unwrap();
    let store_vault_server = StoreVaultServerClient::new(&config.store_vault_server_base_url);
    let user_data = UserData::new(pubkey).encrypt(pubkey);
    store_vault_server.save_user_data(pubkey, user_data).await?;
    Ok(())
}

#[tokio::test]
async fn reset_withdrawal() -> anyhow::Result<()> {
    dotenv::dotenv().ok();
    let config = envy::from_env::<EnvVar>().unwrap();
    let key = generate_key();
    let store_vault_server = StoreVaultServerClient::new(&config.store_vault_server_base_url);
    let user_data_bytes = store_vault_server
        .get_user_data(key.pubkey)
        .await?
        .ok_or(anyhow::anyhow!("User data not found"))?;
    let mut user_data = UserData::decrypt(&user_data_bytes, key)?;

    user_data.processed_withdrawal_uuids = vec![];
    user_data.withdrawal_lpt = 0;

    let user_data_bytes = user_data.encrypt(key.pubkey);
    store_vault_server
        .save_user_data(key.pubkey, user_data_bytes)
        .await?;
    Ok(())
}

fn generate_key() -> KeySet {
    let pubkey: H256 = std::env::var("PUBKEY").unwrap().parse().unwrap();
    let mut rng = rand::thread_rng();
    let mut key = KeySet::rand(&mut rng);
    key.pubkey = BigUint::from_bytes_be(pubkey.as_bytes())
        .try_into()
        .unwrap();
    key
}
