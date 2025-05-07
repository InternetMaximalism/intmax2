use ethers::{
    signers::coins_bip39::{English, Mnemonic},
    types::{Address, H256},
};
use intmax2_client_sdk::{
    client::key_from_eth::generate_intmax_account_from_eth_key,
    external_api::contract::utils::get_address,
};
use intmax2_zkp::{
    common::signature_content::key_set::KeySet,
    ethereum_types::{bytes32::Bytes32, u32limb_trait::U32LimbTrait},
};
use tiny_hderive::bip32::ExtendedPrivKey;

#[derive(Debug, Clone, Copy)]
pub struct Account {
    pub eth_private_key: Bytes32,
    pub eth_address: Address,
    pub intmax_key: KeySet,
}

#[derive(Debug, Clone, Default)]
pub struct MnemonicToPrivateKeyOptions {
    pub account_index: u32,
    pub address_index: u32,
}

pub fn private_key_to_account(eth_private_key: H256) -> Account {
    let chain_id = std::env::var("L1_CHAIN_ID")
        .unwrap()
        .parse::<u64>()
        .expect("Failed to parse L1_CHAIN_ID");
    let intmax_key = generate_intmax_account_from_eth_key(eth_private_key);
    let eth_address = get_address(chain_id, eth_private_key);

    let mut bytes = [0u8; 32];
    let mut i = 0;
    for byte in eth_private_key.as_bytes() {
        if i < 32 {
            bytes[i] = *byte;
            i += 1;
        } else {
            break;
        }
    }

    let eth_private_key = Bytes32::from_bytes_be(&bytes).unwrap();
    Account {
        eth_private_key,
        eth_address,
        intmax_key,
    }
}

pub fn mnemonic_to_private_key(
    mnemonic_phrase: &str,
    options: MnemonicToPrivateKeyOptions,
) -> Result<H256, Box<dyn std::error::Error>> {
    let mnemonic = Mnemonic::<English>::new_from_phrase(mnemonic_phrase)?;
    let seed = mnemonic.to_seed(None)?;

    let account_index = options.account_index;
    let address_index = options.address_index;
    let hd_path = format!("m/44'/60'/{account_index}'/0/{address_index}");

    let master_key = ExtendedPrivKey::derive(&seed, hd_path.as_str())
        .map_err(|e| format!("Failed to derive private key: {e:?}"))?;
    let private_key_bytes = master_key.secret();

    Ok(H256(private_key_bytes))
}

pub fn mnemonic_to_account(
    mnemonic_phrase: &str,
    account_index: u32,
    address_index: u32,
) -> Result<Account, Box<dyn std::error::Error>> {
    let options = MnemonicToPrivateKeyOptions {
        account_index,
        address_index,
    };
    let private_key = mnemonic_to_private_key(mnemonic_phrase, options)?;
    let account = private_key_to_account(private_key);

    Ok(account)
}

pub fn derive_intmax_keys(
    master_mnemonic_phrase: &str,
    num_of_keys: u32,
    offset: u32,
) -> Result<Vec<KeySet>, Box<dyn std::error::Error>> {
    let mut intmax_senders = vec![];
    for address_index in 0..num_of_keys {
        let options = MnemonicToPrivateKeyOptions {
            account_index: 0,
            address_index: offset + address_index,
        };
        let private_key = mnemonic_to_private_key(master_mnemonic_phrase, options)?;
        let key = generate_intmax_account_from_eth_key(private_key);
        intmax_senders.push(key);
    }

    Ok(intmax_senders)
}

pub fn derive_deposit_keys(
    master_mnemonic_phrase: &str,
    num_of_keys: u32,
    offset: u32,
) -> Result<Vec<Account>, Box<dyn std::error::Error>> {
    let mut intmax_senders = vec![];
    for address_index in 0..num_of_keys {
        let options = MnemonicToPrivateKeyOptions {
            account_index: 2,
            address_index: offset + address_index,
        };
        let private_key = mnemonic_to_private_key(master_mnemonic_phrase, options)?;
        let key = private_key_to_account(private_key);
        intmax_senders.push(key);
    }

    Ok(intmax_senders)
}

pub const WITHDRAWAL_ACCOUNT_INDEX: u32 = 3;

pub fn derive_withdrawal_intmax_keys(
    master_mnemonic_phrase: &str,
    num_of_keys: u32,
    offset: u32,
) -> Result<Vec<KeySet>, Box<dyn std::error::Error>> {
    derive_custom_intmax_keys(
        master_mnemonic_phrase,
        WITHDRAWAL_ACCOUNT_INDEX,
        num_of_keys,
        offset,
    )
}

pub fn derive_custom_intmax_keys(
    master_mnemonic_phrase: &str,
    account_index: u32,
    num_of_keys: u32,
    offset: u32,
) -> Result<Vec<KeySet>, Box<dyn std::error::Error>> {
    let mut intmax_senders = vec![];
    for address_index in 0..num_of_keys {
        let options = MnemonicToPrivateKeyOptions {
            account_index,
            address_index: offset + address_index,
        };
        let private_key = mnemonic_to_private_key(master_mnemonic_phrase, options)?;
        let key = generate_intmax_account_from_eth_key(private_key);
        intmax_senders.push(key);
    }

    Ok(intmax_senders)
}

pub fn derive_custom_keys(
    master_mnemonic_phrase: &str,
    account_index: u32,
    num_of_keys: u32,
    offset: u32,
) -> Result<Vec<Account>, Box<dyn std::error::Error>> {
    let mut intmax_senders = vec![];
    for address_index in 0..num_of_keys {
        let options = MnemonicToPrivateKeyOptions {
            account_index,
            address_index: offset + address_index,
        };
        let private_key = mnemonic_to_private_key(master_mnemonic_phrase, options)?;
        let key = private_key_to_account(private_key);
        intmax_senders.push(key);
    }

    Ok(intmax_senders)
}
