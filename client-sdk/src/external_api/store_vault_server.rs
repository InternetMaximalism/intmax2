use async_trait::async_trait;
use intmax2_interfaces::{
    api::{
        error::ServerError,
        store_vault_server::{
            interface::{DataType, SaveDataEntry, StoreVaultClientInterface},
            types::{
                CursorOrder, DataWithMetaData, GetDataBatchRequest, GetDataBatchResponse,
                GetDataSequenceRequest, GetDataSequenceResponse, GetMiscSequenceRequest,
                GetMiscSequenceResponse, GetSenderProofSetRequest, GetSenderProofSetResponse,
                GetUserDataRequest, GetUserDataResponse, MetaDataCursor, MetaDataCursorResponse,
                SaveDataBatchRequest, SaveDataBatchResponse, SaveMiscRequest, SaveMiscResponse,
                SaveSenderProofSetRequest, SaveUserDataRequest,
            },
        },
    },
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
        cursor: &MetaDataCursor,
    ) -> Result<(Vec<DataWithMetaData>, MetaDataCursorResponse), ServerError> {
        let auth = generate_auth_for_get_data_sequence(key);
        let (data, cursor) = self
            .get_data_sequence_with_auth(data_type, cursor, &auth)
            .await?;
        Ok((data, cursor))
    }

    async fn save_misc(
        &self,
        key: KeySet,
        topic: Bytes32,
        encrypted_data: &[u8],
    ) -> Result<String, ServerError> {
        let request = SaveMiscRequest {
            data: encrypted_data.to_vec(),
            topic,
        };
        let request_with_auth = request.sign(key, TIME_TO_EXPIRY);
        let response: SaveMiscResponse = post_request(
            &self.base_url,
            "/store-vault-server/save-misc",
            Some(&request_with_auth),
        )
        .await?;
        Ok(response.uuid)
    }

    async fn get_misc_sequence(
        &self,
        key: KeySet,
        topic: Bytes32,
        meta_cursor: &MetaDataCursor,
    ) -> Result<(Vec<DataWithMetaData>, MetaDataCursorResponse), ServerError> {
        let auth = generate_auth_for_get_misc_sequence(key, topic);
        let (data, cursor) = self
            .get_misc_sequence_native_with_auth(topic, meta_cursor, &auth)
            .await?;
        Ok((data, cursor))
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

    pub async fn get_data_sequence_with_auth(
        &self,
        data_type: DataType,
        cursor: &MetaDataCursor,
        auth: &Auth,
    ) -> Result<(Vec<DataWithMetaData>, MetaDataCursorResponse), ServerError> {
        self.verify_auth_for_get_data_sequence(auth)
            .map_err(|e| ServerError::InvalidAuth(e.to_string()))?;
        let request_with_auth = WithAuth {
            inner: GetDataSequenceRequest {
                data_type,
                cursor: cursor.clone(),
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

    pub async fn get_misc_sequence_native_with_auth(
        &self,
        topic: Bytes32,
        cursor: &MetaDataCursor,
        auth: &Auth,
    ) -> Result<(Vec<DataWithMetaData>, MetaDataCursorResponse), ServerError> {
        let request_with_auth = WithAuth {
            inner: GetMiscSequenceRequest {
                topic,
                cursor: cursor.clone(),
            },
            auth: auth.clone(),
        };
        let response: GetMiscSequenceResponse = post_request(
            &self.base_url,
            "/store-vault-server/get-misc-sequence",
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

pub fn generate_auth_for_get_misc_sequence(key: KeySet, topic: Bytes32) -> Auth {
    // because auth is not dependent on the topic and cursor, we can use a dummy request
    let dummy_request = GetMiscSequenceRequest {
        topic,
        cursor: MetaDataCursor {
            cursor: None,
            order: CursorOrder::Asc,
            limit: None,
        },
    };
    let dummy_request_with_auth = dummy_request.sign(key, TIME_TO_EXPIRY_READONLY);
    dummy_request_with_auth.auth
}
