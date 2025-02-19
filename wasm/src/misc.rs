use ethers::core::k256::sha2::{self, Digest as _};
use intmax2_zkp::ethereum_types::{bytes32::Bytes32, u32limb_trait::U32LimbTrait};
use serde::{Deserialize, Serialize};
use wasm_bindgen::{prelude::wasm_bindgen, JsError};

use crate::{
    client::{get_client, Config},
    init_logger,
    utils::str_privkey_to_keyset,
};
use intmax2_interfaces::{
    api::store_vault_server::{
        interface::StoreVaultClientInterface as _,
        types::{CursorOrder, MetaDataCursor},
    },
    data::{encryption::BlsEncryption as _, generic_misc_data::GenericMiscData},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[wasm_bindgen(getter_with_clone)]
pub struct JsDerive {
    pub derive_path: u32,
    pub redeposit_path: u32,
}

#[wasm_bindgen]
impl JsDerive {
    #[wasm_bindgen(constructor)]
    pub fn new(derive_path: u32, redeposit_path: u32) -> Self {
        Self {
            derive_path,
            redeposit_path,
        }
    }
}

fn derive_topic() -> Bytes32 {
    let result: [u8; 32] = sha2::Sha256::digest(b"derive_path").into();
    Bytes32::from_bytes_be(&result)
}

#[wasm_bindgen]
pub async fn save_derive_path(
    config: &Config,
    private_key: &str,
    derive: &JsDerive,
) -> Result<String, JsError> {
    init_logger();
    let key = str_privkey_to_keyset(private_key)?;
    let client = get_client(config);
    let generic_misc_data = GenericMiscData {
        data: bincode::serialize(derive).unwrap(),
    };
    let encrypted_data = generic_misc_data.encrypt(key.pubkey);
    let uuid = client
        .store_vault_server
        .save_misc(key, derive_topic(), &encrypted_data)
        .await?;
    Ok(uuid)
}

#[wasm_bindgen]
pub async fn get_derive_path_list(
    config: &Config,
    private_key: &str,
) -> Result<Vec<JsDerive>, JsError> {
    init_logger();
    let key = str_privkey_to_keyset(private_key)?;
    let client = get_client(config);

    let mut encrypted_data = vec![];
    let mut cursor = MetaDataCursor {
        cursor: None,
        order: CursorOrder::Asc,
        limit: None,
    };
    loop {
        let (encrypted_data_partial, cursor_response) = client
            .store_vault_server
            .get_misc_sequence(key, derive_topic(), &cursor)
            .await?;
        encrypted_data.extend(encrypted_data_partial);
        if !cursor_response.has_more {
            break;
        }
        cursor.cursor = cursor_response.next_cursor;
    }
    let mut derive_list: Vec<JsDerive> = Vec::new();
    for data in encrypted_data {
        let generic_misc_data = GenericMiscData::decrypt(&data.data, key)?;
        derive_list.push(bincode::deserialize(&generic_misc_data.data).unwrap());
    }
    Ok(derive_list)
}
