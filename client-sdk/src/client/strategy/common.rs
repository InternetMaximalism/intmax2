use intmax2_interfaces::{
    api::store_vault_server::{
        interface::StoreVaultClientInterface,
        types::{DataWithMetaData, MetaDataCursor, MetaDataCursorResponse},
    },
    data::{
        data_type::DataType, encryption::BlsEncryption, meta_data::MetaData,
        sender_proof_set::SenderProofSet, user_data::UserData, validation::Validation,
    },
};
use intmax2_zkp::{
    common::signature::key_set::KeySet,
    ethereum_types::{bytes32::Bytes32, u256::U256},
};
use itertools::Itertools as _;
use num_bigint::BigUint;

use super::error::StrategyError;

pub async fn fetch_decrypt_validate<T: BlsEncryption + Validation>(
    store_vault_server: &dyn StoreVaultClientInterface,
    key: KeySet,
    data_type: DataType,
    included_digests: &[Bytes32],
    excluded_digests: &[Bytes32],
    cursor: &MetaDataCursor,
) -> Result<(Vec<(MetaData, T)>, MetaDataCursorResponse), StrategyError> {
    // fetch pending data
    let encrypted_included_data_with_meta = store_vault_server
        .get_data_batch(key, &data_type.to_topic(), included_digests)
        .await?;

    // fetch unprocessed data
    let (encrypted_unprocessed_data_with_meta, cursor_response) = store_vault_server
        .get_data_sequence(key, &data_type.to_topic(), cursor)
        .await?;

    // decrypt
    let data_with_meta = encrypted_included_data_with_meta
        .into_iter()
        .chain(encrypted_unprocessed_data_with_meta.into_iter())
        .unique_by(|data_with_meta| data_with_meta.meta.digest) // remove duplicates
        .filter_map(|data_with_meta| {
            let DataWithMetaData { meta, data } = data_with_meta;
            if excluded_digests.contains(&meta.digest) {
                log::warn!("{} {} is excluded", data_type, meta.digest);
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
    Ok((data_with_meta, cursor_response))
}

pub async fn fetch_sender_proof_set(
    store_vault_server: &dyn StoreVaultClientInterface,
    ephemeral_key: U256,
) -> Result<SenderProofSet, StrategyError> {
    let key = KeySet::new(BigUint::from(ephemeral_key).into());
    let encrypted_sender_proof_set = store_vault_server
        .get_snapshot(key, &DataType::SenderProofSet.to_topic())
        .await?
        .ok_or(StrategyError::SenderProofSetNotFound)?;
    let sender_proof_set = SenderProofSet::decrypt(&encrypted_sender_proof_set, key)?;
    Ok(sender_proof_set)
}

pub async fn fetch_user_data(
    store_vault_server: &dyn StoreVaultClientInterface,
    key: KeySet,
) -> Result<UserData, StrategyError> {
    let user_data = store_vault_server
        .get_snapshot(key, &DataType::UserData.to_topic())
        .await?
        .map(|encrypted| UserData::decrypt(&encrypted, key))
        .transpose()
        .map_err(|e| StrategyError::UserDataDecryptionError(e.to_string()))?
        .unwrap_or(UserData::new(key.pubkey));
    Ok(user_data)
}
