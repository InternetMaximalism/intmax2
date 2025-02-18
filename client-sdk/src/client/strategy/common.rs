use intmax2_interfaces::{
    api::store_vault_server::{
        interface::{DataType, StoreVaultClientInterface},
        types::DataWithMetaData,
    },
    data::{
        encryption::Encryption,
        meta_data::MetaData,
        sender_proof_set::SenderProofSet,
        user_data::{ProcessStatus, UserData},
        validation::Validation,
    },
};
use intmax2_zkp::{common::signature::key_set::KeySet, ethereum_types::u256::U256};
use itertools::Itertools;
use num_bigint::BigUint;

use super::error::StrategyError;

pub async fn fetch_decrypt_validate<S: StoreVaultClientInterface, T: Encryption + Validation>(
    store_vault_server: &S,
    key: KeySet,
    data_type: DataType,
    process_status: &ProcessStatus,
) -> Result<Vec<(MetaData, T)>, StrategyError> {
    // fetch pending data
    let encrypted_pending_data_with_meta = store_vault_server
        .get_data_batch(key, data_type, &process_status.pending_uuids)
        .await?;

    // fetch unprocessed data
    let encrypted_unprocessed_data_with_meta = store_vault_server
        .get_data_sequence(key, data_type, &process_status.last_processed_meta_data)
        .await?;

    // decrypt
    let data_with_meta = encrypted_pending_data_with_meta
        .into_iter()
        .chain(encrypted_unprocessed_data_with_meta.into_iter())
        .unique_by(|data_with_meta| data_with_meta.meta.uuid.clone()) // remove duplicates
        .filter_map(|data_with_meta| {
            let DataWithMetaData { meta, data } = data_with_meta;
            if process_status.processed_uuids.contains(&meta.uuid) {
                log::warn!("{} {} is already processed", data_type, meta.uuid);
                return None;
            }
            match T::decrypt(&data, key) {
                Ok(data) => match data.validate(key.pubkey) {
                    Ok(_) => Some((meta, data)),
                    Err(e) => {
                        log::warn!("failed to validate {}: {}", data_type, e);
                        None
                    }
                },
                Err(e) => {
                    log::warn!("failed to decrypt {}: {}", data_type, e);
                    None
                }
            }
        })
        .collect::<Vec<_>>();
    Ok(data_with_meta)
}

pub async fn fetch_sender_proof_set<S: StoreVaultClientInterface>(
    store_vault_server: &S,
    ephemeral_key: U256,
) -> Result<SenderProofSet, StrategyError> {
    let key = KeySet::new(BigUint::from(ephemeral_key).into());
    let encrypted_sender_proof_set = store_vault_server.get_sender_proof_set(key).await?;
    let sender_proof_set = SenderProofSet::decrypt(&encrypted_sender_proof_set, key)?;
    Ok(sender_proof_set)
}

pub async fn fetch_user_data<S: StoreVaultClientInterface>(
    store_vault_server: &S,
    key: KeySet,
) -> Result<UserData, StrategyError> {
    let user_data = store_vault_server
        .get_user_data(key)
        .await?
        .map(|encrypted| UserData::decrypt(&encrypted, key))
        .transpose()
        .map_err(|e| StrategyError::UserDataDecryptionError(e.to_string()))?
        .unwrap_or(UserData::new(key.pubkey));
    Ok(user_data)
}
