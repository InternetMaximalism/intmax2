use client::{get_client, Config};
use intmax2_client_sdk::client::key_from_eth::generate_intmax_account_from_eth_key as inner_generate_intmax_account_from_eth_key;
use intmax2_interfaces::{
    api::withdrawal_server::interface::WithdrawalServerClientInterface,
    data::deposit_data::TokenType,
};
use intmax2_zkp::{
    common::transfer::Transfer,
    ethereum_types::{u256::U256, u32limb_trait::U32LimbTrait},
};
use js_types::{
    common::{JsClaimInfo, JsMining, JsTransfer, JsWithdrawalInfo},
    data::{JsDepositResult, JsTxResult, JsUserData},
    fee::{JsFee, JsFeeQuote},
    payment_memo::JsPaymentMemoEntry,
    utils::{parse_address, parse_u256},
    wrapper::JsTxRequestMemo,
};
use num_bigint::BigUint;
use utils::{parse_h256, parse_h256_as_u256, str_privkey_to_keyset};
use wasm_bindgen::{prelude::wasm_bindgen, JsError};

pub mod client;
pub mod fee_payment;
pub mod history;
pub mod js_types;
pub mod misc;
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
#[allow(clippy::too_many_arguments)]
pub async fn send_tx_request(
    config: &Config,
    block_builder_url: &str,
    private_key: &str,
    transfers: Vec<JsTransfer>,
    payment_memos: Vec<JsPaymentMemoEntry>,
    beneficiary: Option<String>,
    fee: Option<JsFee>,
    collateral_fee: Option<JsFee>,
) -> Result<JsTxRequestMemo, JsError> {
    init_logger();
    let key = str_privkey_to_keyset(private_key)?;
    let transfers: Vec<Transfer> = transfers
        .iter()
        .map(|transfer| transfer.clone().try_into())
        .collect::<Result<Vec<_>, JsError>>()?;
    let payment_memos = payment_memos
        .iter()
        .map(|e| e.clone().try_into())
        .collect::<Result<Vec<_>, JsError>>()?;
    let beneficiary = beneficiary.map(|b| parse_h256_as_u256(&b)).transpose()?;
    let fee = fee.map(|f| f.try_into()).transpose()?;
    let collateral_fee = collateral_fee.map(|f| f.try_into()).transpose()?;

    let client = get_client(config);
    let memo = client
        .send_tx_request(
            block_builder_url,
            key,
            transfers,
            payment_memos,
            beneficiary,
            fee,
            collateral_fee,
        )
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
    let proposal = client
        .query_proposal(block_builder_url, key, is_registration_block, tx)
        .await?;
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

/// Resynchronize the user's balance proof.
#[wasm_bindgen]
pub async fn resync(config: &Config, private_key: &str, is_deep: bool) -> Result<(), JsError> {
    init_logger();
    let key = str_privkey_to_keyset(private_key)?;
    let client = get_client(config);
    client.resync(key, is_deep).await?;
    Ok(())
}

/// Synchronize the user's withdrawal proof, and send request to the withdrawal aggregator.
/// It may take a long time to generate ZKP.
#[wasm_bindgen]
pub async fn sync_withdrawals(
    config: &Config,
    private_key: &str,
    fee_token_index: u32,
) -> Result<(), JsError> {
    init_logger();
    let key = str_privkey_to_keyset(private_key)?;
    let client = get_client(config);
    let withdrawal_fee = client.withdrawal_server.get_withdrawal_fee().await?;
    client
        .sync_withdrawals(key, &withdrawal_fee, fee_token_index)
        .await?;
    Ok(())
}

/// Synchronize the user's claim of staking mining, and send request to the withdrawal aggregator.
/// It may take a long time to generate ZKP.
#[wasm_bindgen]
pub async fn sync_claims(
    config: &Config,
    private_key: &str,
    recipient: &str,
    fee_token_index: u32,
) -> Result<(), JsError> {
    init_logger();
    let key = str_privkey_to_keyset(private_key)?;
    let client = get_client(config);
    let recipient = parse_address(recipient)?;
    let claim_fee = client.withdrawal_server.get_claim_fee().await?;
    client
        .sync_claims(key, recipient, &claim_fee, fee_token_index)
        .await?;
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
pub async fn quote_transfer_fee(
    config: &Config,
    block_builder_url: &str,
    pubkey: &str,
    fee_token_index: u32,
) -> Result<JsFeeQuote, JsError> {
    init_logger();
    let pubkey = parse_h256_as_u256(pubkey)?;
    let client = get_client(config);
    let fee_quote = client
        .quote_transfer_fee(block_builder_url, pubkey, fee_token_index)
        .await?;
    Ok(fee_quote.into())
}

#[wasm_bindgen]
pub async fn quote_withdrawal_fee(
    config: &Config,
    withdrawal_token_index: u32,
    fee_token_index: u32,
) -> Result<JsFeeQuote, JsError> {
    init_logger();
    let client = get_client(config);
    let fee_quote = client
        .quote_withdrawal_fee(withdrawal_token_index, fee_token_index)
        .await?;
    Ok(fee_quote.into())
}

#[wasm_bindgen]
pub async fn quote_claim_fee(config: &Config, fee_token_index: u32) -> Result<JsFeeQuote, JsError> {
    init_logger();
    let client = get_client(config);
    let fee_quote = client.quote_claim_fee(fee_token_index).await?;
    Ok(fee_quote.into())
}

fn init_logger() {
    console_error_panic_hook::set_once();
    // wasm_logger::init(wasm_logger::Config::default());
}
