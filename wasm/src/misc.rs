use intmax2_zkp::ethereum_types::u32limb_trait::U32LimbTrait;
use serde::{Deserialize, Serialize};
use wasm_bindgen::{prelude::wasm_bindgen, JsError};

use crate::{
    client::{get_client, Config},
    init_logger,
    utils::str_to_view_pair,
};
use intmax2_interfaces::{
    api::store_vault_server::{
        interface::SaveDataEntry,
        types::{CursorOrder, MetaDataCursor},
    },
    data::{
        encryption::BlsEncryption as _,
        generic_misc_data::GenericMiscData,
        rw_rights::{RWRights, ReadRights, WriteRights},
        topic::topic_from_rights,
    },
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

fn derive_path_topic() -> String {
    topic_from_rights(
        RWRights {
            read_rights: ReadRights::AuthRead,
            write_rights: WriteRights::AuthWrite,
        },
        "derive_path",
    )
}

#[wasm_bindgen]
pub async fn save_derive_path(
    config: &Config,
    view_pair: &str,
    derive: &JsDerive,
) -> Result<String, JsError> {
    init_logger();
    let view_pair = str_to_view_pair(view_pair)?;
    let view_pub = view_pair.view.to_public_key();

    let client = get_client(config);
    let generic_misc_data = GenericMiscData {
        data: bincode::serialize(derive).unwrap(),
    };

    let entry = SaveDataEntry {
        topic: derive_path_topic(),
        pubkey: view_pub.0,
        data: generic_misc_data.encrypt(view_pub, Some(view_pair.view))?,
    };
    let digests = client
        .store_vault_server
        .save_data_batch(view_pair.view, &[entry])
        .await?;
    Ok(digests[0].to_hex())
}

#[wasm_bindgen]
pub async fn get_derive_path_list(
    config: &Config,
    view_pair: &str,
) -> Result<Vec<JsDerive>, JsError> {
    init_logger();
    let view_pair = str_to_view_pair(view_pair)?;
    let view_pub = view_pair.view.to_public_key();
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
            .get_data_sequence(view_pair.view, &derive_path_topic(), &cursor)
            .await?;
        encrypted_data.extend(encrypted_data_partial);
        if !cursor_response.has_more {
            break;
        }
        cursor.cursor = cursor_response.next_cursor;
    }
    let mut derive_list: Vec<JsDerive> = Vec::new();
    for data in encrypted_data {
        let generic_misc_data =
            GenericMiscData::decrypt(view_pair.view, Some(view_pub), &data.data)?;
        derive_list.push(bincode::deserialize(&generic_misc_data.data).unwrap());
    }
    Ok(derive_list)
}
