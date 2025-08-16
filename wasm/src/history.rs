use crate::{
    client::{get_client, Config},
    init_logger,
    js_types::{
        cursor::{JsMetaDataCursor, JsMetaDataCursorResponse},
        history::{JsDepositEntry, JsTransferEntry, JsTxEntry},
        utils::parse_bytes32,
    },
    utils::str_to_view_pair,
};
use intmax2_interfaces::api::store_vault_server::types::MetaDataCursor;
use intmax2_zkp::ethereum_types::bytes32::Bytes32;
use wasm_bindgen::{prelude::wasm_bindgen, JsError, JsValue};

#[wasm_bindgen(getter_with_clone)]
pub struct JsDepositHistory {
    pub history: Vec<JsDepositEntry>,
    pub cursor_response: JsMetaDataCursorResponse,
}

#[wasm_bindgen(getter_with_clone)]
pub struct JsTransferHistory {
    pub history: Vec<JsTransferEntry>,
    pub cursor_response: JsMetaDataCursorResponse,
}

#[wasm_bindgen(getter_with_clone)]
pub struct JsTxHistory {
    pub history: Vec<JsTxEntry>,
    pub cursor_response: JsMetaDataCursorResponse,
}

#[wasm_bindgen]
pub async fn fetch_deposit_history(
    config: &Config,
    view_pair: &str,
    cursor: &JsMetaDataCursor,
) -> Result<JsDepositHistory, JsError> {
    init_logger();
    let cursor: MetaDataCursor = cursor.clone().try_into()?;
    let view_pair = str_to_view_pair(view_pair)?;
    let client = get_client(config);
    let (history, cursor_response) = client.fetch_deposit_history(view_pair, &cursor).await?;
    let js_history = history.into_iter().map(JsDepositEntry::from).collect();
    let js_cursor_response = JsMetaDataCursorResponse::from(cursor_response);
    Ok(JsDepositHistory {
        history: js_history,
        cursor_response: js_cursor_response,
    })
}

#[wasm_bindgen]
pub async fn fetch_transfer_history(
    config: &Config,
    view_pair: &str,
    cursor: &JsMetaDataCursor,
) -> Result<JsTransferHistory, JsError> {
    init_logger();

    let cursor: MetaDataCursor = cursor.clone().try_into()?;
    let view_pair = str_to_view_pair(view_pair)?;
    let client = get_client(config);
    let (history, cursor_response) = client.fetch_transfer_history(view_pair, &cursor).await?;
    let js_history = history.into_iter().map(JsTransferEntry::from).collect();
    let js_cursor_response = JsMetaDataCursorResponse::from(cursor_response);
    Ok(JsTransferHistory {
        history: js_history,
        cursor_response: js_cursor_response,
    })
}

#[wasm_bindgen]
pub async fn fetch_tx_history(
    config: &Config,
    view_pair: &str,
    cursor: &JsMetaDataCursor,
) -> Result<JsTxHistory, JsError> {
    init_logger();
    let cursor: MetaDataCursor = cursor.clone().try_into()?;
    let view_pair = str_to_view_pair(view_pair)?;
    let client = get_client(config);
    let (history, cursor_response) = client.fetch_tx_history(view_pair, &cursor).await?;
    let js_history = history.into_iter().map(JsTxEntry::from).collect();
    let js_cursor_response = JsMetaDataCursorResponse::from(cursor_response);
    Ok(JsTxHistory {
        history: js_history,
        cursor_response: js_cursor_response,
    })
}

#[wasm_bindgen]
pub async fn fetch_deposit_batch(
    config: &Config,
    view_pair: &str,
    digests: &JsValue, // an array of Bytes32
) -> Result<Vec<JsDepositEntry>, JsError> {
    init_logger();
    let digests: Vec<Bytes32> = parse_digests(digests)?;
    let view_pair = str_to_view_pair(view_pair)?;
    let client = get_client(config);
    let history = client.fetch_deposit_batch(view_pair, &digests).await?;
    let js_history = history.into_iter().map(JsDepositEntry::from).collect();
    Ok(js_history)
}

#[wasm_bindgen]
pub async fn fetch_transfer_batch(
    config: &Config,
    view_pair: &str,
    digests: &JsValue, // an array of Bytes32
) -> Result<Vec<JsTransferEntry>, JsError> {
    init_logger();
    let digests: Vec<Bytes32> = parse_digests(digests)?;
    let view_pair = str_to_view_pair(view_pair)?;
    let client = get_client(config);
    let history = client.fetch_transfer_batch(view_pair, &digests).await?;
    let js_history = history.into_iter().map(JsTransferEntry::from).collect();
    Ok(js_history)
}

#[wasm_bindgen]
pub async fn fetch_tx_batch(
    config: &Config,
    view_pair: &str,
    digests: &JsValue, // an array of Bytes32
) -> Result<Vec<JsTxEntry>, JsError> {
    init_logger();
    let digests: Vec<Bytes32> = parse_digests(digests)?;
    let view_pair = str_to_view_pair(view_pair)?;
    let client = get_client(config);
    let history = client.fetch_tx_batch(view_pair, &digests).await?;
    let js_history = history.into_iter().map(JsTxEntry::from).collect();
    Ok(js_history)
}

fn parse_digests(digests: &JsValue) -> Result<Vec<Bytes32>, JsError> {
    let digests: Vec<String> = serde_wasm_bindgen::from_value(digests.clone())
        .map_err(|e| JsError::new(&format!("failed to deserialize digests: {e}")))?;
    let digests = digests
        .into_iter()
        .map(|d| parse_bytes32(&d))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(digests)
}
