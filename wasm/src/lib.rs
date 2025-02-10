use client::{get_client, Config};
use intmax2_client_sdk::{
    client::key_from_eth::generate_intmax_account_from_eth_key as inner_generate_intmax_account_from_eth_key,
    external_api::utils::time::sleep_for,
};
use intmax2_interfaces::data::deposit_data::TokenType;
use intmax2_zkp::{
    common::transfer::Transfer,
    constants::NUM_TRANSFERS_IN_TX,
    ethereum_types::{u256::U256, u32limb_trait::U32LimbTrait},
};
use js_types::{
    common::{JsClaimInfo, JsMining, JsTransfer, JsWithdrawalInfo},
    data::{JsDepositResult, JsTxResult, JsUserData},
    history::{JsDepositEntry, JsTransferEntry, JsTxEntry},
    utils::{parse_address, parse_u256},
    wrapper::JsTxRequestMemo,
};
use num_bigint::BigUint;
use utils::{parse_h256, parse_h256_as_u256, str_privkey_to_keyset};
use wasm_bindgen::{prelude::wasm_bindgen, JsError};

pub mod client;
pub mod js_types;
pub mod native;
pub mod utils;

#[derive(Debug, Clone)]
#[wasm_bindgen(getter_with_clone)]
pub struct IntmaxAccount {
    pub privkey: String,
    pub pubkey: String,
}

/// Generate a new key pair from the given ethereum private key (32bytes hex string).
#[wasm_bindgen]
pub async fn generate_intmax_account_from_eth_key(
    eth_private_key: &str,
) -> Result<IntmaxAccount, JsError> {
    init_logger();
    let eth_private_key = parse_h256(eth_private_key)?;
    let key_set = inner_generate_intmax_account_from_eth_key(eth_private_key);
    let private_key: U256 = BigUint::from(key_set.privkey).try_into().unwrap();
    Ok(IntmaxAccount {
        privkey: private_key.to_hex(),
        pubkey: key_set.pubkey.to_hex(),
    })
}

/// Function to take a backup before calling the deposit function of the liquidity contract.
/// You can also get the pubkey_salt_hash from the return value.
#[wasm_bindgen]
#[allow(clippy::too_many_arguments)]
pub async fn prepare_deposit(
    config: &Config,
    depositor: &str,
    recipient: &str,
    amount: &str,
    token_type: u8,
    token_address: &str,
    token_id: &str,
    is_mining: bool,
) -> Result<JsDepositResult, JsError> {
    init_logger();
    let depositor = parse_address(depositor)?;
    let recipient = parse_h256_as_u256(recipient)?;
    let amount = parse_u256(amount)?;
    let token_type = TokenType::try_from(token_type).map_err(|e| JsError::new(&e))?;
    let token_address = parse_address(token_address)?;
    let token_id = parse_u256(token_id)?;
    let client = get_client(config);
    let deposit_result = client
        .prepare_deposit(
            depositor,
            recipient,
            amount,
            token_type,
            token_address,
            token_id,
            is_mining,
        )
        .await
        .map_err(|e| JsError::new(&format!("failed to prepare deposit call: {}", e)))?;
    Ok(deposit_result.into())
}

/// Function to send a tx request to the block builder. The return value contains information to take a backup.
#[wasm_bindgen]
pub async fn send_tx_request(
    config: &Config,
    block_builder_url: &str,
    private_key: &str,
    transfers: Vec<JsTransfer>,
    fee_token_index: u32,
) -> Result<JsTxRequestMemo, JsError> {
    init_logger();
    if transfers.len() > NUM_TRANSFERS_IN_TX {
        return Err(JsError::new(&format!(
            "Number of transfers in a tx must be less than or equal to {}",
            NUM_TRANSFERS_IN_TX
        )));
    }
    let key = str_privkey_to_keyset(private_key)?;
    let transfers: Vec<Transfer> = transfers
        .iter()
        .map(|transfer| transfer.clone().try_into())
        .collect::<Result<Vec<_>, JsError>>()?;

    let client = get_client(config);
    let memo = client
        .send_tx_request(block_builder_url, key, transfers, fee_token_index)
        .await
        .map_err(|e| JsError::new(&format!("failed to send tx request {}", e)))?;

    Ok(JsTxRequestMemo::from_tx_request_memo(&memo))
}

/// Function to query the block proposal from the block builder, and
/// send the signed tx tree root to the block builder during taking a backup of the tx.
#[wasm_bindgen]
pub async fn query_and_finalize(
    config: &Config,
    block_builder_url: &str,
    private_key: &str,
    tx_request_memo: &JsTxRequestMemo,
) -> Result<JsTxResult, JsError> {
    init_logger();
    let key = str_privkey_to_keyset(private_key)?;
    let client = get_client(config);
    let tx_request_memo = tx_request_memo.to_tx_request_memo()?;
    let is_registration_block = tx_request_memo.is_registration_block;
    let tx = tx_request_memo.tx;
    let mut tries = 0;
    let proposal = loop {
        let proposal = client
            .query_proposal(block_builder_url, key, is_registration_block, tx)
            .await?;
        if let Some(p) = proposal {
            break p;
        }
        if tries > config.block_builder_query_limit {
            return Err(JsError::new("Failed to get proposal"));
        }
        tries += 1;
        sleep_for(config.block_builder_query_interval).await;
    };
    let tx_result = client
        .finalize_tx(block_builder_url, key, &tx_request_memo, &proposal)
        .await?;
    Ok(tx_result.into())
}

/// Synchronize the user's balance proof. It may take a long time to generate ZKP.
#[wasm_bindgen]
pub async fn sync(config: &Config, private_key: &str) -> Result<(), JsError> {
    init_logger();
    let key = str_privkey_to_keyset(private_key)?;
    let client = get_client(config);
    client.sync(key).await?;
    Ok(())
}

/// Synchronize the user's withdrawal proof, and send request to the withdrawal aggregator.
/// It may take a long time to generate ZKP.
#[wasm_bindgen]
pub async fn sync_withdrawals(config: &Config, private_key: &str) -> Result<(), JsError> {
    init_logger();
    let key = str_privkey_to_keyset(private_key)?;
    let client = get_client(config);
    client.sync_withdrawals(key).await?;
    Ok(())
}

/// Synchronize the user's claim of staking mining, and send request to the withdrawal aggregator.
/// It may take a long time to generate ZKP.
#[wasm_bindgen]
pub async fn sync_claims(
    config: &Config,
    private_key: &str,
    recipient: &str,
) -> Result<(), JsError> {
    init_logger();
    let key = str_privkey_to_keyset(private_key)?;
    let client = get_client(config);
    let recipient = parse_address(recipient)?;
    client.sync_claims(key, recipient).await?;
    Ok(())
}

/// Get the user's data. It is recommended to sync before calling this function.
#[wasm_bindgen]
pub async fn get_user_data(config: &Config, private_key: &str) -> Result<JsUserData, JsError> {
    init_logger();
    let key = str_privkey_to_keyset(private_key)?;
    let client = get_client(config);
    let user_data = client.get_user_data(key).await?;
    Ok(user_data.into())
}

#[wasm_bindgen]
pub async fn get_withdrawal_info(
    config: &Config,
    private_key: &str,
) -> Result<Vec<JsWithdrawalInfo>, JsError> {
    init_logger();
    let key = str_privkey_to_keyset(private_key)?;
    let client = get_client(config);
    let info = client.get_withdrawal_info(key).await?;
    let js_info = info.into_iter().map(JsWithdrawalInfo::from).collect();
    Ok(js_info)
}

#[wasm_bindgen]
pub async fn get_withdrawal_info_by_recipient(
    config: &Config,
    recipient: &str,
) -> Result<Vec<JsWithdrawalInfo>, JsError> {
    init_logger();
    let client = get_client(config);
    let recipient = parse_address(recipient)?;
    let info = client.get_withdrawal_info_by_recipient(recipient).await?;
    let js_info = info.into_iter().map(JsWithdrawalInfo::from).collect();
    Ok(js_info)
}

#[wasm_bindgen]
pub async fn get_mining_list(config: &Config, private_key: &str) -> Result<Vec<JsMining>, JsError> {
    init_logger();
    let key = str_privkey_to_keyset(private_key)?;
    let client = get_client(config);
    let minings = client.get_mining_list(key).await?;
    let js_minings = minings.into_iter().map(JsMining::from).collect();
    Ok(js_minings)
}

#[wasm_bindgen]
pub async fn get_claim_info(
    config: &Config,
    private_key: &str,
) -> Result<Vec<JsClaimInfo>, JsError> {
    init_logger();
    let key = str_privkey_to_keyset(private_key)?;
    let client = get_client(config);
    let info = client.get_claim_info(key).await?;
    let js_info = info.into_iter().map(JsClaimInfo::from).collect();
    Ok(js_info)
}

#[wasm_bindgen]
pub async fn fetch_deposit_history(
    config: &Config,
    private_key: &str,
) -> Result<Vec<JsDepositEntry>, JsError> {
    init_logger();
    let key = str_privkey_to_keyset(private_key)?;
    let client = get_client(config);
    let history = client.fetch_deposit_history(key).await?;
    let js_history = history.into_iter().map(JsDepositEntry::from).collect();
    Ok(js_history)
}

#[wasm_bindgen]
pub async fn fetch_transfer_history(
    config: &Config,
    private_key: &str,
) -> Result<Vec<JsTransferEntry>, JsError> {
    init_logger();
    let key = str_privkey_to_keyset(private_key)?;
    let client = get_client(config);
    let history = client.fetch_transfer_history(key).await?;
    let js_history = history.into_iter().map(JsTransferEntry::from).collect();
    Ok(js_history)
}

#[wasm_bindgen]
pub async fn fetch_tx_history(
    config: &Config,
    private_key: &str,
) -> Result<Vec<JsTxEntry>, JsError> {
    init_logger();
    let key = str_privkey_to_keyset(private_key)?;
    let client = get_client(config);
    let history = client.fetch_tx_history(key).await?;
    let js_history = history.into_iter().map(JsTxEntry::from).collect();
    Ok(js_history)
}

fn init_logger() {
    console_error_panic_hook::set_once();
    // wasm_logger::init(wasm_logger::Config::default());
}
