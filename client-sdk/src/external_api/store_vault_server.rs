use async_trait::async_trait;
use intmax2_interfaces::{
    api::{
        error::ServerError,
        store_vault_server::{
            interface::{DataType, SaveDataEntry, StoreVaultClientInterface},
            types::{
                CursorOrder, DataWithMetaData, GetDataBatchRequest, GetDataBatchResponse,
                GetDataSequenceRequest, GetDataSequenceResponse, GetSenderProofSetRequest,
                GetSenderProofSetResponse, GetUserDataRequest, GetUserDataResponse, MetaDataCursor,
                MetaDataCursorResponse, SaveDataBatchRequest, SaveDataBatchResponse,
                SaveSenderProofSetRequest, SaveUserDataRequest,
            },
        },
    },
    data::meta_data::MetaData,
    utils::signature::{Auth, Signable, WithAuth},
};
use intmax2_zkp::{common::signature::key_set::KeySet, ethereum_types::bytes32::Bytes32};

use super::utils::query::post_request;

const TIME_TO_EXPIRY: u64 = 60; // 1 minute for normal requests
const TIME_TO_EXPIRY_READONLY: u64 = 60 * 60 * 24; // 24 hours for readonly

#[derive(Debug, Clone)]
pub struct StoreVaultServerClient {
    base_url: String,
}

impl StoreVaultServerClient {
    pub fn new(base_url: &str) -> Self {
        StoreVaultServerClient {
            base_url: base_url.to_string(),
        }
    }
}

#[async_trait(?Send)]
impl StoreVaultClientInterface for StoreVaultServerClient {
    async fn save_user_data(
        &self,
        key: KeySet,
        prev_digest: Option<Bytes32>,
        encrypted_data: &[u8],
    ) -> Result<(), ServerError> {
        let request = SaveUserDataRequest {
            data: encrypted_data.to_vec(),
            prev_digest,
        };
        let request_with_auth = request.sign(key, TIME_TO_EXPIRY);
        post_request::<_, ()>(
            &self.base_url,
            "/store-vault-server/save-user-data",
            Some(&request_with_auth),
        )
        .await?;
        Ok(())
    }

    async fn get_user_data(&self, key: KeySet) -> Result<Option<Vec<u8>>, ServerError> {
        let request = GetUserDataRequest;
        let request_with_auth = request.sign(key, TIME_TO_EXPIRY);
        let response: GetUserDataResponse = post_request(
            &self.base_url,
            "/store-vault-server/get-user-data",
            Some(&request_with_auth),
        )
        .await?;
        Ok(response.data)
    }

    async fn save_sender_proof_set(
        &self,
        ephemeral_key: KeySet,
        encrypted_data: &[u8],
    ) -> Result<(), ServerError> {
        let request = SaveSenderProofSetRequest {
            data: encrypted_data.to_vec(),
        };
        let request_with_auth = request.sign(ephemeral_key, TIME_TO_EXPIRY);
        post_request::<_, ()>(
            &self.base_url,
            "/store-vault-server/save-sender-proof-set",
            Some(&request_with_auth),
        )
        .await?;
        Ok(())
    }

    async fn get_sender_proof_set(&self, ephemeral_key: KeySet) -> Result<Vec<u8>, ServerError> {
        let request = GetSenderProofSetRequest;
        let request_with_auth = request.sign(ephemeral_key, TIME_TO_EXPIRY);
        let response: GetSenderProofSetResponse = post_request(
            &self.base_url,
            "/store-vault-server/get-sender-proof-set",
            Some(&request_with_auth),
        )
        .await?;
        Ok(response.data)
    }

    async fn save_data_batch(
        &self,
        key: KeySet,
        entries: &[SaveDataEntry],
    ) -> Result<Vec<String>, ServerError> {
        let request = SaveDataBatchRequest {
            data: entries.to_vec(),
        };
        let request_with_auth = request.sign(key, TIME_TO_EXPIRY);
        let response: SaveDataBatchResponse = post_request(
            &self.base_url,
            "/store-vault-server/save-data-batch",
            Some(&request_with_auth),
        )
        .await?;
        Ok(response.uuids)
    }

    async fn get_data_batch(
        &self,
        key: KeySet,
        data_type: DataType,
        uuids: &[String],
    ) -> Result<Vec<DataWithMetaData>, ServerError> {
        let request = GetDataBatchRequest {
            data_type,
            uuids: uuids.to_vec(),
        };
        let request_with_auth = request.sign(key, TIME_TO_EXPIRY);
        let response: GetDataBatchResponse = post_request(
            &self.base_url,
            "/store-vault-server/get-data-batch",
            Some(&request_with_auth),
        )
        .await?;
        Ok(response.data)
    }

    async fn get_data_sequence(
        &self,
        key: KeySet,
        data_type: DataType,
        metadata_cursor: &Option<MetaData>,
    ) -> Result<Vec<DataWithMetaData>, ServerError> {
        let mut data_array = vec![];

        let mut has_more = true;
        let mut metadata_cursor = metadata_cursor.clone();
        let auth = generate_auth_for_get_data_sequence(key);
        while has_more {
            let (data, cursor) = self
                .get_data_sequence_native(
                    data_type,
                    &metadata_cursor,
                    &None,
                    &CursorOrder::Asc,
                    &auth,
                )
                .await?;
            has_more = cursor.has_more;
            metadata_cursor = cursor.next_cursor;
            data_array.extend(data);
        }
        Ok(data_array)
    }
}

impl StoreVaultServerClient {
    fn verify_auth_for_get_data_sequence(&self, auth: &Auth) -> anyhow::Result<()> {
        let dummy_request = GetDataSequenceRequest {
            data_type: DataType::Deposit,
            cursor: MetaDataCursor {
                cursor: None,
                order: CursorOrder::Asc,
                limit: None,
            },
        };
        dummy_request.verify(auth)
    }

    pub async fn get_data_sequence_native(
        &self,
        data_type: DataType,
        metadata_cursor: &Option<MetaData>,
        limit: &Option<u32>,
        order: &CursorOrder,
        auth: &Auth,
    ) -> Result<(Vec<DataWithMetaData>, MetaDataCursorResponse), ServerError> {
        self.verify_auth_for_get_data_sequence(auth)
            .map_err(|e| ServerError::InvalidAuth(e.to_string()))?;
        let request_with_auth = WithAuth {
            inner: GetDataSequenceRequest {
                data_type,
                cursor: MetaDataCursor {
                    cursor: metadata_cursor.clone(),
                    order: order.clone(),
                    limit: *limit,
                },
            },
            auth: auth.clone(),
        };
        let response: GetDataSequenceResponse = post_request(
            &self.base_url,
            "/store-vault-server/get-data-sequence",
            Some(&request_with_auth),
        )
        .await?;
        Ok((response.data, response.cursor_response))
    }
}

pub fn generate_auth_for_get_data_sequence(key: KeySet) -> Auth {
    // because auth is not dependent on the datatype and cursor, we can use a dummy request
    let dummy_request = GetDataSequenceRequest {
        data_type: DataType::Deposit,
        cursor: MetaDataCursor {
            cursor: None,
            order: CursorOrder::Asc,
            limit: None,
        },
    };
    let dummy_request_with_auth = dummy_request.sign(key, TIME_TO_EXPIRY_READONLY);
    dummy_request_with_auth.auth
}
