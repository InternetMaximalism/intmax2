use crate::{
    js_types::{
        client::{JsDepositResult, JsTransferRequest, JsTxResult},
        utils::{parse_intmax_address, parse_network, parse_public_key},
    },
    utils::str_to_key_pair,
};
use client::{get_client, Config};
use intmax2_client_sdk::client::types::{PaymentMemoEntry, TransferFeeQuote, TransferRequest};
use intmax2_interfaces::{
    data::deposit_data::TokenType,
    utils::{
        address::IntmaxAddress,
        key::ViewPair,
        key_derivation::{derive_keypair_from_spend_key, derive_spend_key_from_bytes32},
    },
};
use intmax2_zkp::{
    common::deposit::Deposit, ethereum_types::u32limb_trait::U32LimbTrait,
    utils::leafable::Leafable,
};
use js_types::{
    common::{JsClaimInfo, JsMining, JsWithdrawalInfo},
    data::{balances_to_token_balances, JsTransferData, JsUserData, TokenBalance},
    fee::{JsFeeQuote, JsTransferFeeQuote},
    payment_memo::JsPaymentMemoEntry,
    utils::{parse_address, parse_bytes32, parse_u256},
    wrapper::JsTxRequestMemo,
};
use utils::str_to_view_pair;
use wasm_bindgen::{prelude::wasm_bindgen, JsError, JsValue};

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
    pub address: String,
    pub view_pair: String,
    pub key_pair: String,
    pub spend_key: String,
    pub spend_pub: String,
}

/// Generate a new key pair from the given ethereum private key (32bytes hex string).
#[wasm_bindgen]
pub async fn generate_intmax_account_from_eth_key(
    network: &str,
    eth_private_key: &str,
    is_legacy: bool,
) -> Result<IntmaxAccount, JsError> {
    init_logger();
    let network = parse_network(network)?;
    let eth_private_key = parse_bytes32(eth_private_key)?;
    let spend_key = derive_spend_key_from_bytes32(eth_private_key);
    let key_pair = derive_keypair_from_spend_key(spend_key, is_legacy);
    let view_pair: ViewPair = key_pair.into();
    let address = IntmaxAddress::from_viewpair(network, &view_pair);

    Ok(IntmaxAccount {
        address: address.to_string(),
        view_pair: view_pair.to_string(),
        key_pair: key_pair.to_string(),
        spend_key: spend_key.to_string(),
        spend_pub: view_pair.spend.to_string(),
    })
}

/// Get the hash of the deposit.
#[wasm_bindgen]
pub fn get_deposit_hash(
    depositor: &str,
    recipient_salt_hash: &str,
    token_index: u32,
    amount: &str,
    is_eligible: bool,
) -> Result<String, JsError> {
    init_logger();
    let depositor = parse_address(depositor)?;
    let recipient_salt_hash = parse_bytes32(recipient_salt_hash)?;
    let amount = parse_u256(amount)?;
    let is_eligible = is_eligible as u32;

    let deposit = Deposit {
        depositor,
        pubkey_salt_hash: recipient_salt_hash,
        amount,
        token_index,
        is_eligible: is_eligible != 0,
    };
    let deposit_hash = deposit.hash();
    Ok(deposit_hash.to_hex())
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
    let recipient = parse_intmax_address(recipient)?;
    let amount = parse_u256(amount)?;
    let token_type = TokenType::try_from(token_type).map_err(|e| JsError::new(&e))?;
    let token_address = parse_address(token_address)?;
    let token_id = parse_u256(token_id)?;
    let client = get_client(config);
    let deposit_result = client
        .prepare_deposit(
            depositor,
            recipient.to_public_keypair(),
            amount,
            token_type,
            token_address,
            token_id,
            is_mining,
        )
        .await
        .map_err(|e| JsError::new(&format!("failed to prepare deposit call: {e}")))?;
    Ok(deposit_result.into())
}

/// Wait for the tx to be sendable. Wait for the sync of validity prover and balance proof.
#[wasm_bindgen]
pub async fn await_tx_sendable(
    config: &Config,
    view_pair: &str,
    transfer_requests: &JsValue, // same as Vec<JsTransferRequest> but use JsValue to avoid moving the ownership
    fee_quote: &JsTransferFeeQuote, // same as Vec<JsPaymentMemoEntry> but use JsValue to avoid moving the ownership
) -> Result<(), JsError> {
    init_logger();
    let transfer_requests: Vec<JsTransferRequest> =
        serde_wasm_bindgen::from_value(transfer_requests.clone())
            .map_err(|e| JsError::new(&format!("failed to deserialize transfer requests: {e}")))?;
    let transfer_requests: Vec<TransferRequest> = transfer_requests
        .iter()
        .map(|transfer| transfer.clone().try_into())
        .collect::<Result<Vec<_>, JsError>>()?;

    let view_pair = str_to_view_pair(view_pair)?;
    let fee_quote: TransferFeeQuote = fee_quote.clone().try_into()?;
    let client = get_client(config);
    client
        .await_tx_sendable(view_pair, &transfer_requests, &fee_quote)
        .await?;
    Ok(())
}

/// Function to send a tx request to the block builder. The return value contains information to take a backup.
#[wasm_bindgen]
pub async fn send_tx_request(
    config: &Config,
    block_builder_url: &str,
    key_pair: &str,
    transfer_requests: &JsValue, // same as Vec<JsTransferRequest> but use JsValue to avoid moving the ownership
    payment_memos: &JsValue, // same as Vec<JsPaymentMemoEntry> but use JsValue to avoid moving the ownership
    fee_quote: &JsTransferFeeQuote,
) -> Result<JsTxRequestMemo, JsError> {
    init_logger();
    let key_pair = str_to_key_pair(key_pair)?;
    let transfer_requests: Vec<JsTransferRequest> =
        serde_wasm_bindgen::from_value(transfer_requests.clone())
            .map_err(|e| JsError::new(&format!("failed to deserialize transfer requests: {e}")))?;
    let transfer_requests: Vec<TransferRequest> = transfer_requests
        .iter()
        .map(|transfer| transfer.clone().try_into())
        .collect::<Result<Vec<_>, JsError>>()?;
    let payment_memos: Vec<JsPaymentMemoEntry> =
        serde_wasm_bindgen::from_value(payment_memos.clone())
            .map_err(|e| JsError::new(&format!("failed to deserialize payment memos: {e}")))?;
    let payment_memos: Vec<PaymentMemoEntry> = payment_memos
        .iter()
        .map(|e| e.clone().try_into())
        .collect::<Result<Vec<_>, JsError>>()?;

    let fee_quote: TransferFeeQuote = fee_quote.clone().try_into()?;
    let client = get_client(config);
    let memo = client
        .send_tx_request(
            block_builder_url,
            key_pair,
            &transfer_requests,
            &payment_memos,
            &fee_quote,
        )
        .await
        .map_err(|e| JsError::new(&format!("failed to send tx request {e}")))?;

    Ok(JsTxRequestMemo::from_tx_request_memo(&memo))
}

/// Function to query the block proposal from the block builder, and
/// send the signed tx tree root to the block builder during taking a backup of the tx.
#[wasm_bindgen]
pub async fn query_and_finalize(
    config: &Config,
    block_builder_url: &str,
    key_pair: &str,
    tx_request_memo: &JsTxRequestMemo,
) -> Result<JsTxResult, JsError> {
    init_logger();
    let key_pair = str_to_key_pair(key_pair)?;
    let client = get_client(config);
    let tx_request_memo = tx_request_memo.to_tx_request_memo()?;
    let proposal = client
        .query_proposal(block_builder_url, &tx_request_memo.request_id)
        .await?;
    let tx_result = client
        .finalize_tx(block_builder_url, key_pair, &tx_request_memo, &proposal)
        .await?;
    Ok(tx_result.into())
}

#[wasm_bindgen]
pub async fn get_tx_status(
    config: &Config,
    pubkey: &str,
    tx_tree_root: &str,
) -> Result<String, JsError> {
    init_logger();
    let client = get_client(config);
    let pubkey = parse_public_key(pubkey)?;
    let tx_tree_root = parse_bytes32(tx_tree_root)?;
    let status = client
        .get_tx_status(pubkey, tx_tree_root)
        .await
        .map_err(|e| JsError::new(&format!("failed to get tx status: {e}")))?;
    Ok(status.to_string())
}

/// Synchronize the user's balance proof. It may take a long time to generate ZKP.
#[wasm_bindgen]
pub async fn sync(config: &Config, view_pair: &str) -> Result<(), JsError> {
    init_logger();
    let view_pair = str_to_view_pair(view_pair)?;
    let client = get_client(config);
    client.sync(view_pair).await?;
    Ok(())
}

/// Resynchronize the user's balance proof.
#[wasm_bindgen]
pub async fn resync(config: &Config, view_pair: &str, is_deep: bool) -> Result<(), JsError> {
    init_logger();
    let view_pair = str_to_view_pair(view_pair)?;
    let client = get_client(config);
    client.resync(view_pair, is_deep).await?;
    Ok(())
}

/// Synchronize the user's withdrawal proof, and send request to the withdrawal aggregator.
/// It may take a long time to generate ZKP.
#[wasm_bindgen]
pub async fn sync_withdrawals(
    config: &Config,
    view_pair: &str,
    fee_token_index: u32,
) -> Result<(), JsError> {
    init_logger();
    let view_pair = str_to_view_pair(view_pair)?;
    let client = get_client(config);
    let withdrawal_fee = client.withdrawal_server.get_withdrawal_fee().await?;
    client
        .sync_withdrawals(view_pair, &withdrawal_fee, fee_token_index)
        .await?;
    Ok(())
}

/// Synchronize the user's claim of staking mining, and send request to the withdrawal aggregator.
/// It may take a long time to generate ZKP.
#[wasm_bindgen]
pub async fn sync_claims(
    config: &Config,
    view_pair: &str,
    recipient: &str,
    fee_token_index: u32,
) -> Result<(), JsError> {
    init_logger();
    let view_pair = str_to_view_pair(view_pair)?;
    let client = get_client(config);
    let recipient = parse_address(recipient)?;
    let claim_fee = client.withdrawal_server.get_claim_fee().await?;
    client
        .sync_claims(view_pair, recipient, &claim_fee, fee_token_index)
        .await?;
    Ok(())
}

/// Get the user's data. It is recommended to sync before calling this function.
#[wasm_bindgen]
pub async fn get_user_data(config: &Config, view_pair: &str) -> Result<JsUserData, JsError> {
    init_logger();
    let view_pair = str_to_view_pair(view_pair)?;
    let client = get_client(config);
    let user_data = client.get_user_data(view_pair).await?;
    Ok(user_data.into())
}

#[wasm_bindgen]
pub async fn get_withdrawal_info(
    config: &Config,
    view_pair: &str,
) -> Result<Vec<JsWithdrawalInfo>, JsError> {
    init_logger();
    let view_pair = str_to_view_pair(view_pair)?;
    let client = get_client(config);
    let info = client.get_withdrawal_info(view_pair.view).await?;
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
pub async fn get_mining_list(config: &Config, view_pair: &str) -> Result<Vec<JsMining>, JsError> {
    init_logger();
    let view_pair = str_to_view_pair(view_pair)?;
    let client = get_client(config);
    let minings = client.get_mining_list(view_pair).await?;
    let js_minings = minings.into_iter().map(JsMining::from).collect();
    Ok(js_minings)
}

#[wasm_bindgen]
pub async fn get_claim_info(config: &Config, view_pair: &str) -> Result<Vec<JsClaimInfo>, JsError> {
    init_logger();
    let view_pair = str_to_view_pair(view_pair)?;
    let client = get_client(config);
    let info = client.get_claim_info(view_pair.view).await?;
    let js_info = info.into_iter().map(JsClaimInfo::from).collect();
    Ok(js_info)
}

#[wasm_bindgen]
pub async fn quote_transfer_fee(
    config: &Config,
    block_builder_url: &str,
    pubkey: &str,
    fee_token_index: u32,
) -> Result<JsTransferFeeQuote, JsError> {
    init_logger();
    let pubkey = parse_bytes32(pubkey)?.into();
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

#[wasm_bindgen]
pub async fn make_history_backup(
    config: &Config,
    view_pair: &str,
    from: u64,
    chunk_size: u32,
) -> Result<Vec<String>, JsError> {
    init_logger();
    let view_pair = str_to_view_pair(view_pair)?;
    let client = get_client(config);
    let csvs = client
        .make_history_backup(view_pair, from, chunk_size as usize)
        .await?;
    Ok(csvs)
}

#[wasm_bindgen]
pub async fn generate_transfer_receipt(
    config: &Config,
    view_pair: &str,
    tx_digest: &str,
    transfer_index: u32,
) -> Result<String, JsError> {
    init_logger();
    let view_pair = str_to_view_pair(view_pair)?;
    let transfer_digest = parse_bytes32(tx_digest)?;
    let client = get_client(config);
    let receipt = client
        .generate_transfer_receipt(view_pair, transfer_digest, transfer_index)
        .await?;
    Ok(receipt)
}

#[wasm_bindgen]
pub async fn validate_transfer_receipt(
    config: &Config,
    view_pair: &str,
    transfer_receipt: &str,
) -> Result<JsTransferData, JsError> {
    init_logger();
    let view_pair = str_to_view_pair(view_pair)?;
    let client = get_client(config);
    let transfer_data = client
        .validate_transfer_receipt(view_pair, transfer_receipt)
        .await?;
    Ok(transfer_data.into())
}

#[wasm_bindgen]
pub async fn get_balances_without_sync(
    config: &Config,
    view_pair: &str,
) -> Result<Vec<TokenBalance>, JsError> {
    init_logger();
    let view_pair = str_to_view_pair(view_pair)?;
    let client = get_client(config);
    let balances = client.get_balances_without_sync(view_pair).await?;
    Ok(balances_to_token_balances(balances))
}

#[wasm_bindgen]
pub async fn check_validity_prover(config: &Config) -> Result<(), JsError> {
    init_logger();
    let client = get_client(config);
    client.check_validity_prover().await?;
    Ok(())
}

fn init_logger() {
    console_error_panic_hook::set_once();
    // wasm_logger::init(wasm_logger::Config::default());
}
