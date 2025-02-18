use ethers::abi::AbiEncode;
use intmax2_zkp::ethereum_types::u32limb_trait::U32LimbTrait;
use tests::{
    accounts::{
        derive_deposit_keys, derive_intmax_keys, derive_withdrawal_intmax_keys,
        mnemonic_to_private_key, MnemonicToPrivateKeyOptions,
    },
    EnvVar,
};

async fn test_derive_intmax_account(address_type: &str) -> Result<(), Box<dyn std::error::Error>> {
    dotenv::dotenv()?;
    dotenv::from_path("../cli/.env")?;
    let config = envy::from_env::<EnvVar>().unwrap();
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .init();

    let master_mnemonic = config.master_mnemonic;

    match address_type {
        "transfer" => {
            for address_index in 0..10 {
                let key = derive_intmax_keys(&master_mnemonic, 1, address_index)?;
                let private_key: num_bigint::BigUint = key[0].privkey.into();
                println!("address index: {}", address_index);
                println!("INTMAX private key: {:#064x}", private_key);
                println!("INTMAX address: {}", key[0].pubkey);
                println!("INTMAX address (hex): {}", key[0].pubkey.to_hex());
            }
        }
        "withdrawal" => {
            for address_index in 0..100 {
                // let key = derive_intmax_keys(&master_mnemonic, 1, address_index)?;
                let options = MnemonicToPrivateKeyOptions {
                    account_index: 3,
                    address_index,
                };
                let eth_private_key = mnemonic_to_private_key(&master_mnemonic, options)?;
                let key = derive_withdrawal_intmax_keys(&master_mnemonic, 1, address_index)?;
                let private_key: num_bigint::BigUint = key[0].privkey.into();
                println!("address index: {}", address_index);
                println!("Ethereum private key: {}", eth_private_key.encode_hex());
                println!("INTMAX private key: {:#064x}", private_key);
                println!("INTMAX address: {}", key[0].pubkey);
                println!("INTMAX address (hex): {}", key[0].pubkey.to_hex());
            }
        }
        "deposit" => {
            for address_index in 0..100 {
                let key = derive_deposit_keys(&master_mnemonic, 1, address_index)?;
                let private_key: num_bigint::BigUint = key[0].intmax_key.privkey.into();
                println!("address index: {}", address_index);
                println!(
                    "Ethereum private key: {}",
                    key[0].eth_private_key.encode_hex()
                );
                println!("INTMAX private key: {:#064x}", private_key);
                println!("INTMAX address: {}", key[0].intmax_key.pubkey);
                println!(
                    "INTMAX address (hex): {}",
                    key[0].intmax_key.pubkey.to_hex()
                );
            }
        }
        _ => {}
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();

    let transfer_address_type = "transfer".to_string();
    let address_type = args.get(1).unwrap_or(&transfer_address_type);

    // let address_index = args.get(2).unwrap_or(&0).parse::<u32>().unwrap();

    test_derive_intmax_account(address_type).await
}

#[cfg(test)]
mod test {
    use ethers::{types::H256, utils::hex};
    use intmax2_cli::format::privkey_to_keyset;
    use intmax2_client_sdk::{
        client::key_from_eth::generate_intmax_account_from_eth_key,
        external_api::contract::utils::get_address,
    };
    use intmax2_zkp::ethereum_types::u32limb_trait::U32LimbTrait;
    use tests::EnvVar;

    #[ignore]
    #[tokio::test]
    async fn test_generate_intmax_account_from_eth_key() -> Result<(), Box<dyn std::error::Error>> {
        dotenv::dotenv()?;
        dotenv::from_path("../cli/.env")?;
        let config = envy::from_env::<EnvVar>().unwrap();
        env_logger::builder()
            .filter_level(log::LevelFilter::Info)
            .init();

        let private_key = H256::from_slice(&hex::decode(config.private_key)?); // Ethereum private key
        let key = generate_intmax_account_from_eth_key(private_key);
        println!("INTMAX address: {}", key.pubkey);
        println!("INTMAX address (hex): {}", key.pubkey.to_hex());

        let address = get_address(1, private_key);
        println!("Ethereum Address: {:?}", address);

        Ok(())
    }

    #[ignore]
    #[tokio::test]
    async fn test_calculate_intmax_account() -> Result<(), Box<dyn std::error::Error>> {
        dotenv::dotenv()?;
        dotenv::from_path("../cli/.env")?;
        let config = envy::from_env::<EnvVar>().unwrap();
        env_logger::builder()
            .filter_level(log::LevelFilter::Info)
            .init();

        let private_key = H256::from_slice(&hex::decode(config.private_key)?); // INTMAX private key
        let key = privkey_to_keyset(private_key);
        println!("INTMAX address: {}", key.pubkey);
        println!("INTMAX address (hex): {}", key.pubkey.to_hex());

        Ok(())
    }
}
