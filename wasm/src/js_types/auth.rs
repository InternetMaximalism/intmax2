use intmax2_interfaces::utils::signature::Auth;
use intmax2_zkp::{
    common::signature::flatten::FlatG2,
    ethereum_types::{u256::U256, u32limb_trait::U32LimbTrait},
};
use wasm_bindgen::prelude::wasm_bindgen;

#[derive(Debug, Clone)]
#[wasm_bindgen(getter_with_clone)]
pub struct JsFlatG2 {
    pub elements: Vec<String>, // hex string
}

impl From<FlatG2> for JsFlatG2 {
    fn from(flat_g2: FlatG2) -> Self {
        Self {
            elements: flat_g2.0.iter().map(|e| e.to_hex()).collect(),
        }
    }
}

impl TryFrom<JsFlatG2> for FlatG2 {
    type Error = &'static str;

    fn try_from(js_flat_g2: JsFlatG2) -> Result<Self, Self::Error> {
        let elements = js_flat_g2
            .elements
            .iter()
            .map(|e| U256::from_hex(e).map_err(|_| "Invalid hex string"))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self(elements.try_into().map_err(|_| "Invalid length")?))
    }
}

#[derive(Debug, Clone)]
#[wasm_bindgen(getter_with_clone)]
pub struct JsAuth {
    pub pubkey: String, // hex string
    pub expiry: u64,
    pub signature: JsFlatG2, // hex string
}

impl From<Auth> for JsAuth {
    fn from(auth: Auth) -> Self {
        Self {
            pubkey: auth.pubkey.to_hex(),
            expiry: auth.expiry,
            signature: auth.signature.into(),
        }
    }
}

impl TryFrom<JsAuth> for Auth {
    type Error = &'static str;

    fn try_from(js_auth: JsAuth) -> Result<Self, Self::Error> {
        Ok(Self {
            pubkey: U256::from_hex(&js_auth.pubkey).map_err(|_| "Invalid hex string")?,
            expiry: js_auth.expiry,
            signature: FlatG2::try_from(js_auth.signature)?,
        })
    }
}
