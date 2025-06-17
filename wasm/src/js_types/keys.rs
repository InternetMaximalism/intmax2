use intmax2_interfaces::utils::key::PublicKeyPair;
use wasm_bindgen::{prelude::wasm_bindgen, JsError};

#[derive(Debug, Clone)]
#[wasm_bindgen(getter_with_clone)]
pub struct JsPublicKeyPair {
    pub view: String,
    pub spend: String,
}

#[wasm_bindgen]
impl JsPublicKeyPair {
    #[wasm_bindgen(constructor)]
    pub fn new(view: String, spend: String) -> Self {
        JsPublicKeyPair { view, spend }
    }

    pub fn to_string(&self) -> Result<String, JsError> {
        let keys: PublicKeyPair = self.clone().try_into()?;
        Ok(keys.to_string())
    }

    pub fn from_string(keys: String) -> Result<JsPublicKeyPair, JsError> {
        let keys: PublicKeyPair = keys.parse().map_err(|_| JsError::new("Invalid key pair"))?;
        Ok(keys.into())
    }
}

impl From<PublicKeyPair> for JsPublicKeyPair {
    fn from(keys: PublicKeyPair) -> Self {
        JsPublicKeyPair {
            view: keys.view.to_string(),
            spend: keys.spend.to_string(),
        }
    }
}

impl TryFrom<JsPublicKeyPair> for PublicKeyPair {
    type Error = JsError;

    fn try_from(js_keys: JsPublicKeyPair) -> Result<Self, Self::Error> {
        let view = js_keys
            .view
            .parse()
            .map_err(|_| wasm_bindgen::JsError::new("Invalid view key"))?;
        let spend = js_keys
            .spend
            .parse()
            .map_err(|_| wasm_bindgen::JsError::new("Invalid spend key"))?;
        Ok(PublicKeyPair { view, spend })
    }
}
