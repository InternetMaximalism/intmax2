use intmax2_interfaces::{
    api::store_vault_server::types::{MetaDataCursor, MetaDataCursorResponse},
    data::meta_data::MetaData,
};
use wasm_bindgen::{prelude::wasm_bindgen, JsError};

use super::common::JsMetaData;

#[derive(Debug, Default, Clone)]
#[wasm_bindgen(getter_with_clone)]
pub struct JsMetaDataCursor {
    pub cursor: Option<JsMetaData>,
    pub order: String,
    pub limit: Option<u32>,
}

#[wasm_bindgen]
impl JsMetaDataCursor {
    #[wasm_bindgen(constructor)]
    pub fn new(cursor: Option<JsMetaData>, order: String, limit: Option<u32>) -> Self {
        Self {
            cursor,
            order,
            limit,
        }
    }
}

impl From<MetaDataCursor> for JsMetaDataCursor {
    fn from(cursor: MetaDataCursor) -> Self {
        Self {
            cursor: cursor.cursor.map(JsMetaData::from),
            order: cursor.order.to_string(),
            limit: cursor.limit,
        }
    }
}

impl TryFrom<JsMetaDataCursor> for MetaDataCursor {
    type Error = JsError;

    fn try_from(cursor: JsMetaDataCursor) -> Result<Self, Self::Error> {
        Ok(Self {
            cursor: cursor.cursor.map(MetaData::from),
            order: cursor
                .order
                .parse()
                .map_err(|e| JsError::new(&format!("Failed to parse CursorOrder: {}", e)))?,
            limit: cursor.limit,
        })
    }
}

#[derive(Debug, Default, Clone)]
#[wasm_bindgen(getter_with_clone)]
pub struct JsMetaDataCursorResponse {
    pub next_cursor: Option<JsMetaData>,
    pub has_more: bool,
    pub total_count: u32,
}

#[wasm_bindgen]
impl JsMetaDataCursorResponse {
    #[wasm_bindgen(constructor)]
    pub fn new(next_cursor: Option<JsMetaData>, has_more: bool, total_count: u32) -> Self {
        Self {
            next_cursor,
            has_more,
            total_count,
        }
    }
}

impl From<MetaDataCursorResponse> for JsMetaDataCursorResponse {
    fn from(cursor_response: MetaDataCursorResponse) -> Self {
        Self {
            next_cursor: cursor_response.next_cursor.map(JsMetaData::from),
            has_more: cursor_response.has_more,
            total_count: cursor_response.total_count,
        }
    }
}

impl From<JsMetaDataCursorResponse> for MetaDataCursorResponse {
    fn from(cursor_response: JsMetaDataCursorResponse) -> Self {
        Self {
            next_cursor: cursor_response.next_cursor.map(MetaData::from),
            has_more: cursor_response.has_more,
            total_count: cursor_response.total_count,
        }
    }
}
