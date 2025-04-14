use intmax2_client_sdk::client::multisig;
use intmax2_interfaces::data::encryption::bls::v1::multisig as multisig_encryption;
use intmax2_zkp::{
    common::signature_content::flatten::FlatG2,
    ethereum_types::{u256::U256, u32limb_trait::U32LimbTrait},
};
use std::convert::TryFrom;
use wasm_bindgen::{prelude::wasm_bindgen, JsError};

use super::auth::JsFlatG2;

impl From<multisig_encryption::MultiEciesStep2Response> for JsMultiEciesStep2Response {
    fn from(response: multisig_encryption::MultiEciesStep2Response) -> Self {
        let (server_ecdh_share, y_parity) = response.server_ecdh_share;
        Self {
            server_ecdh_share: JsEllipticCurvePoint {
                x: server_ecdh_share.to_hex(),
                y_parity,
            },
            server_pubkey: response.server_pubkey.to_hex(),
        }
    }
}

#[derive(Debug, Clone)]
#[wasm_bindgen(getter_with_clone)]
pub struct JsMultiEciesStep1Response {
    pub encrypted_data: Vec<u8>,
    pub client_pubkey: String, // hex string
}

impl From<multisig_encryption::MultiEciesStep1Response> for JsMultiEciesStep1Response {
    fn from(response: multisig_encryption::MultiEciesStep1Response) -> Self {
        Self {
            encrypted_data: response.encrypted_data,
            client_pubkey: response.client_pubkey.to_hex(),
        }
    }
}

impl TryFrom<&JsMultiEciesStep1Response> for multisig_encryption::MultiEciesStep1Response {
    type Error = JsError;

    fn try_from(response: &JsMultiEciesStep1Response) -> Result<Self, Self::Error> {
        let client_pubkey = U256::from_hex(&response.client_pubkey).map_err(|_| {
            JsError::new("Failed to parse client public key in decrypt_bls_interaction_step1")
        })?;
        Ok(Self {
            encrypted_data: response.encrypted_data.clone(),
            client_pubkey,
        })
    }
}

#[derive(Debug, Clone)]
#[wasm_bindgen(getter_with_clone)]
pub struct JsEllipticCurvePoint {
    pub x: String, // hex string
    pub y_parity: bool,
}

#[derive(Debug, Clone)]
#[wasm_bindgen(getter_with_clone)]
pub struct JsMultiEciesStep2Response {
    pub server_ecdh_share: JsEllipticCurvePoint,
    pub server_pubkey: String, // hex string
}

impl TryFrom<&JsMultiEciesStep2Response> for multisig_encryption::MultiEciesStep2Response {
    type Error = JsError;

    fn try_from(response: &JsMultiEciesStep2Response) -> Result<Self, Self::Error> {
        let server_ecdh_share = U256::from_hex(&response.server_ecdh_share.x).map_err(|_| {
            JsError::new("Failed to parse server ECDH share in decrypt_bls_interaction_step2")
        })?;
        let server_pubkey = U256::from_hex(&response.server_pubkey).map_err(|_| {
            JsError::new("Failed to parse server public key in decrypt_bls_interaction_step2")
        })?;
        Ok(Self {
            server_ecdh_share: (server_ecdh_share, response.server_ecdh_share.y_parity),
            server_pubkey,
        })
    }
}

#[derive(Debug, Clone)]
#[wasm_bindgen(getter_with_clone)]
pub struct JsMultiEciesStep3Response {
    pub message: Vec<u8>,
}

impl From<multisig_encryption::MultiEciesStep3Response> for JsMultiEciesStep3Response {
    fn from(response: multisig_encryption::MultiEciesStep3Response) -> Self {
        Self {
            message: response.message,
        }
    }
}

impl TryFrom<&JsMultiEciesStep3Response> for multisig_encryption::MultiEciesStep3Response {
    type Error = JsError;

    fn try_from(response: &JsMultiEciesStep3Response) -> Result<Self, Self::Error> {
        Ok(Self {
            message: response.message.clone(),
        })
    }
}

#[derive(Debug, Clone)]
#[wasm_bindgen(getter_with_clone)]
pub struct JsMultisigStep1Response {
    pub client_pubkey: String, // hex string
    pub message: Vec<u8>,
}

impl From<multisig::MultisigStep1Response> for JsMultisigStep1Response {
    fn from(response: multisig::MultisigStep1Response) -> Self {
        Self {
            client_pubkey: response.client_pubkey.to_hex(),
            message: response.message,
        }
    }
}

impl TryFrom<&JsMultisigStep1Response> for multisig::MultisigStep1Response {
    type Error = JsError;

    fn try_from(response: &JsMultisigStep1Response) -> Result<Self, Self::Error> {
        let client_pubkey = U256::from_hex(&response.client_pubkey).map_err(|_| {
            JsError::new("Failed to parse client public key in decrypt_bls_interaction_step1")
        })?;
        Ok(Self {
            client_pubkey,
            message: response.message.clone(),
        })
    }
}

#[derive(Debug, Clone)]
#[wasm_bindgen(getter_with_clone)]
pub struct JsMultisigStep2Response {
    pub server_signature: JsFlatG2,
    pub server_pubkey: String, // hex string
}

impl From<multisig::MultisigStep2Response> for JsMultisigStep2Response {
    fn from(response: multisig::MultisigStep2Response) -> Self {
        let server_signature = FlatG2::from(response.server_signature);
        Self {
            server_signature: server_signature.into(),
            server_pubkey: response.server_pubkey.to_hex(),
        }
    }
}

impl TryFrom<&JsMultisigStep2Response> for multisig::MultisigStep2Response {
    type Error = JsError;

    fn try_from(response: &JsMultisigStep2Response) -> Result<Self, Self::Error> {
        let server_signature = FlatG2::try_from(response.server_signature.clone())
            .map_err(|_| JsError::new("Failed to parse server signature"))?;
        let server_pubkey = U256::from_hex(&response.server_pubkey).map_err(|_| {
            JsError::new("Failed to parse server public key in decrypt_bls_interaction_step2")
        })?;
        Ok(Self {
            server_signature: server_signature.into(),
            server_pubkey,
        })
    }
}

#[derive(Debug, Clone)]
#[wasm_bindgen(getter_with_clone)]
pub struct JsMultisigStep3Response {
    pub aggregated_signature: JsFlatG2,
    pub aggregated_pubkey: String, // hex string
}

impl From<multisig::MultisigStep3Response> for JsMultisigStep3Response {
    fn from(response: multisig::MultisigStep3Response) -> Self {
        let aggregated_signature = FlatG2::from(response.aggregated_signature);
        Self {
            aggregated_signature: aggregated_signature.into(),
            aggregated_pubkey: response.aggregated_pubkey.to_hex(),
        }
    }
}

impl TryFrom<&JsMultisigStep3Response> for multisig::MultisigStep3Response {
    type Error = JsError;

    fn try_from(response: &JsMultisigStep3Response) -> Result<Self, Self::Error> {
        let aggregated_signature = FlatG2::try_from(response.aggregated_signature.clone())
            .map_err(|_| JsError::new("Failed to parse aggregated signature"))?;
        let aggregated_pubkey = U256::from_hex(&response.aggregated_pubkey).map_err(|_| {
            JsError::new("Failed to parse aggregated public key in decrypt_bls_interaction_step3")
        })?;
        Ok(Self {
            aggregated_signature: aggregated_signature.into(),
            aggregated_pubkey,
        })
    }
}
