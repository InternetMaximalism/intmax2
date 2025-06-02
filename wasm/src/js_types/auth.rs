use intmax2_interfaces::utils::signature::Auth;
use intmax2_zkp::{
    common::signature_content::flatten::FlatG2,
    ethereum_types::{u256::U256, u32limb_trait::U32LimbTrait},
};
use wasm_bindgen::prelude::wasm_bindgen;

const ERR_INVALID_HEX: &str = "Invalid hex string";
const ERR_INVALID_LENGTH: &str = "Invalid length";

#[derive(Debug, Clone)]
#[wasm_bindgen(getter_with_clone)]
pub struct JsFlatG2 {
    pub elements: Vec<String>, // hex string
}

#[wasm_bindgen]
impl JsFlatG2 {
    #[wasm_bindgen(constructor)]
    pub fn new(elements: Vec<String>) -> Self {
        Self { elements }
    }
}

impl From<FlatG2> for JsFlatG2 {
    fn from(flat_g2: FlatG2) -> Self {
        Self {
            elements: flat_g2.0.iter().map(|e| e.to_hex()).collect(),
        }
    }
}

impl From<&FlatG2> for JsFlatG2 {
    fn from(flat_g2: &FlatG2) -> Self {
        Self {
            elements: flat_g2.0.iter().map(|e| e.to_hex()).collect(),
        }
    }
}

impl TryFrom<JsFlatG2> for FlatG2 {
    type Error = &'static str;

    fn try_from(js_flat_g2: JsFlatG2) -> Result<Self, Self::Error> {
        if js_flat_g2.elements.len() != 4 {
            return Err(ERR_INVALID_LENGTH);
        }
        let array = js_flat_g2
            .elements
            .iter()
            .map(|e| U256::from_hex(e).map_err(|_| ERR_INVALID_HEX))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Self(array.try_into().unwrap()))
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
            signature: JsFlatG2::from(auth.signature),
        }
    }
}

impl From<&Auth> for JsAuth {
    fn from(auth: &Auth) -> Self {
        Self {
            pubkey: auth.pubkey.to_hex(),
            expiry: auth.expiry,
            signature: JsFlatG2::from(&auth.signature),
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

#[cfg(test)]
mod tests {
    use super::*;
    use intmax2_zkp::ethereum_types::u256::U256;

    fn valid_u256_hex_strings() -> Vec<String> {
        vec![
            "0x01".to_string(),
            "0x02".to_string(),
            "0x03".to_string(),
            "0x04".to_string(),
        ]
    }

    #[test]
    fn test_jsflatg2_to_flatg2_round_trip() {
        let js_flat = JsFlatG2::new(valid_u256_hex_strings());
        let flat: FlatG2 = FlatG2::try_from(js_flat.clone()).expect("Conversion should succeed");
        let js_flat_back: JsFlatG2 = flat.into();

        let expected_flat: FlatG2 = FlatG2::try_from(js_flat).unwrap();
        let actual_flat: FlatG2 = FlatG2::try_from(js_flat_back).unwrap();

        assert_eq!(expected_flat, actual_flat);
    }

    #[test]
    fn test_flatg2_to_jsflatg2_round_trip() {
        let original_flat = FlatG2([U256::from(1), U256::from(2), U256::from(3), U256::from(4)]);

        // Convert FlatG2 -> JsFlatG2 -> FlatG2
        let js_flat: JsFlatG2 = JsFlatG2::from(original_flat.clone());
        let converted_flat: FlatG2 = FlatG2::try_from(js_flat).expect("Conversion should succeed");

        assert_eq!(original_flat, converted_flat);
    }

    #[test]
    fn test_invalid_hex_string() {
        let invalid_elements = vec![
            "0x01".to_string(),
            "0x02".to_string(),
            "not_hex".to_string(),
            "0x04".to_string(),
        ];
        let js_flat = JsFlatG2::new(invalid_elements);
        let result = FlatG2::try_from(js_flat);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), ERR_INVALID_HEX);
    }

    #[test]
    fn test_invalid_element_count() {
        let too_few = JsFlatG2::new(vec![
            "0x01".to_string(),
            "0x02".to_string(),
            "0x03".to_string(),
        ]);
        let result = FlatG2::try_from(too_few);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), ERR_INVALID_LENGTH);
    }

    #[test]
    fn test_too_many_elements() {
        let elements = (1..=5).map(|n| format!("{n:#066x}")).collect();
        let js_flat = JsFlatG2::new(elements);
        let result = FlatG2::try_from(js_flat);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), ERR_INVALID_LENGTH);
    }

    #[test]
    fn test_empty_elements() {
        let js_flat = JsFlatG2::new(vec![]);
        let result = FlatG2::try_from(js_flat);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), ERR_INVALID_LENGTH);
    }

    #[test]
    fn test_jsauth_round_trip() {
        let auth = Auth {
            pubkey: U256::from(12345),
            expiry: 999999,
            signature: FlatG2([U256::from(1), U256::from(2), U256::from(3), U256::from(4)]),
        };

        let js_auth: JsAuth = JsAuth::from(auth.clone());
        let auth_back = Auth::try_from(js_auth).expect("JsAuth to Auth conversion should succeed");

        assert_eq!(auth, auth_back);
    }

    #[test]
    fn test_jsauth_invalid_pubkey() {
        let js_auth = JsAuth {
            pubkey: "not_hex".to_string(),
            expiry: 123,
            signature: JsFlatG2::new((1..=4).map(|n| format!("{n:#066x}")).collect()),
        };

        let result = Auth::try_from(js_auth);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Invalid hex string");
    }
}
