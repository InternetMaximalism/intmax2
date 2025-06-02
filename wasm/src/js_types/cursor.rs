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

impl TryFrom<&JsMetaDataCursor> for MetaDataCursor {
    type Error = JsError;

    fn try_from(cursor: &JsMetaDataCursor) -> Result<Self, Self::Error> {
        Ok(Self {
            cursor: cursor.cursor.as_ref().map(MetaData::try_from).transpose()?,
            order: cursor
                .order
                .parse()
                .map_err(|e| JsError::new(&format!("Failed to parse CursorOrder: {e}")))?,
            limit: cursor.limit,
        })
    }
}

impl TryFrom<JsMetaDataCursor> for MetaDataCursor {
    type Error = JsError;

    fn try_from(cursor: JsMetaDataCursor) -> Result<Self, Self::Error> {
        Self::try_from(&cursor)
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

impl TryFrom<&JsMetaDataCursorResponse> for MetaDataCursorResponse {
    type Error = JsError;

    fn try_from(cursor_response: &JsMetaDataCursorResponse) -> Result<Self, Self::Error> {
        Ok(Self {
            next_cursor: cursor_response
                .next_cursor
                .as_ref()
                .map(MetaData::try_from)
                .transpose()?,
            has_more: cursor_response.has_more,
            total_count: cursor_response.total_count,
        })
    }
}

impl TryFrom<JsMetaDataCursorResponse> for MetaDataCursorResponse {
    type Error = JsError;

    fn try_from(cursor_response: JsMetaDataCursorResponse) -> Result<Self, Self::Error> {
        Self::try_from(&cursor_response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use intmax2_interfaces::api::store_vault_server::types::MetaDataCursorResponse;

    fn dummy_js_meta_data() -> JsMetaData {
        JsMetaData {
            timestamp: 0,
            digest: "".to_string(),
        }
    }

    #[test]
    fn test_js_meta_data_cursor_response_roundtrip() {
        let js_resp = JsMetaDataCursorResponse {
            next_cursor: Some(dummy_js_meta_data()),
            has_more: true,
            total_count: 42,
        };

        let meta_resp = MetaDataCursorResponse::try_from(js_resp.clone())
            .expect("MetaDataCursorResponse::try_from should succeed");

        assert_eq!(meta_resp.has_more, js_resp.has_more);
        assert_eq!(meta_resp.total_count, js_resp.total_count);

        let js_resp_back = JsMetaDataCursorResponse::from(meta_resp);

        assert_eq!(js_resp_back.has_more, js_resp.has_more);
        assert_eq!(js_resp_back.total_count, js_resp.total_count);
    }

    #[test]
    fn test_js_meta_data_cursor_response_try_from_none_next_cursor() {
        let js_resp = JsMetaDataCursorResponse {
            next_cursor: None,
            has_more: false,
            total_count: 0,
        };

        let meta_resp = MetaDataCursorResponse::try_from(js_resp.clone())
            .expect("MetaDataCursorResponse::try_from should succeed");

        assert_eq!(meta_resp.next_cursor, None);
        assert!(!meta_resp.has_more);
        assert_eq!(meta_resp.total_count, 0);
    }
}
