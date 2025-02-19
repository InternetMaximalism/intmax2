use crate::{
    client::{get_client, Config},
    init_logger,
    js_types::{
        cursor::{JsMetaDataCursor, JsMetaDataCursorResponse},
        history::{JsDepositEntry, JsTransferEntry, JsTxEntry},
    },
    utils::str_privkey_to_keyset,
};
use intmax2_interfaces::api::store_vault_server::types::MetaDataCursor;
use wasm_bindgen::{prelude::wasm_bindgen, JsError};

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
    private_key: &str,
    cursor: &JsMetaDataCursor,
) -> Result<JsDepositHistory, JsError> {
    init_logger();

    let cursor: MetaDataCursor = cursor.clone().try_into()?;
    let key = str_privkey_to_keyset(private_key)?;
    let client = get_client(config);
    let (history, cursor_response) = client.fetch_deposit_history(key, &cursor).await?;
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
    private_key: &str,
    cursor: &JsMetaDataCursor,
) -> Result<JsTransferHistory, JsError> {
    init_logger();

    let cursor: MetaDataCursor = cursor.clone().try_into()?;
    let key = str_privkey_to_keyset(private_key)?;
    let client = get_client(config);
    let (history, cursor_response) = client.fetch_transfer_history(key, &cursor).await?;
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
    private_key: &str,
    cursor: &JsMetaDataCursor,
) -> Result<JsTxHistory, JsError> {
    init_logger();

    let cursor: MetaDataCursor = cursor.clone().try_into()?;
    let key = str_privkey_to_keyset(private_key)?;
    let client = get_client(config);
    let (history, cursor_response) = client.fetch_tx_history(key, &cursor).await?;
    let js_history = history.into_iter().map(JsTxEntry::from).collect();
    let js_cursor_response = JsMetaDataCursorResponse::from(cursor_response);
    Ok(JsTxHistory {
        history: js_history,
        cursor_response: js_cursor_response,
    })
}
