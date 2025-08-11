use intmax2_interfaces::{
    api::store_vault_server::{
        interface::StoreVaultClientInterface,
        types::{DataWithMetaData, MetaDataCursor, MetaDataCursorResponse},
    },
    data::{
        data_type::DataType, encryption::BlsEncryption, meta_data::MetaData,
        rw_rights::WriteRights, sender_proof_set::SenderProofSet, user_data::UserData,
        validation::Validation,
    },
    utils::key::{PrivateKey, ViewPair},
};
use intmax2_zkp::ethereum_types::bytes32::Bytes32;
use itertools::Itertools as _;

use super::error::StrategyError;

pub async fn fetch_decrypt_validate<T: BlsEncryption + Validation>(
    store_vault_server: &dyn StoreVaultClientInterface,
    view_priv: PrivateKey,
    data_type: DataType,
    included_digests: &[Bytes32],
    excluded_digests: &[Bytes32],
    cursor: Option<&MetaDataCursor>,
) -> Result<(Vec<(MetaData, T)>, Option<MetaDataCursorResponse>), StrategyError> {
    // fetch pending data
    let mut all_encrypted_data_with_meta = store_vault_server
        .get_data_batch(view_priv, &data_type.to_topic(), included_digests)
        .await?;

    // fetch unprocessed data
    let cursor_response = if let Some(cursor) = cursor {
        let (encrypted_unprocessed_data_with_meta, cursor_response) = store_vault_server
            .get_data_sequence(view_priv, &data_type.to_topic(), cursor)
            .await?;
        all_encrypted_data_with_meta.extend(encrypted_unprocessed_data_with_meta);
        Some(cursor_response)
    } else {
        None
    };

    // decrypt
    let data_with_meta = all_encrypted_data_with_meta
        .into_iter()
        .unique_by(|data_with_meta| data_with_meta.meta.digest) // remove duplicates
        .filter_map(|data_with_meta| {
            let DataWithMetaData { meta, data } = data_with_meta;
            if excluded_digests.contains(&meta.digest) {
                log::warn!("{} {} is excluded", data_type, meta.digest);
                return None;
            }
            let enc_sender = match data_type.rw_rights().write_rights {
                WriteRights::SingleAuthWrite => Some(view_priv.to_public_key()),
                WriteRights::AuthWrite => Some(view_priv.to_public_key()),
                WriteRights::SingleOpenWrite => None,
                WriteRights::OpenWrite => None,
            };
            match T::decrypt(view_priv, enc_sender, &data) {
                Ok(data) => match data.validate() {
                    Ok(_) => Some((meta, data)),
                    Err(e) => {
                        log::warn!("failed to validate {data_type}: {e}");
                        None
                    }
                },
                Err(e) => {
                    log::warn!("failed to decrypt {data_type}: {e}");
                    None
                }
            }
        })
        .collect::<Vec<_>>();
    Ok((data_with_meta, cursor_response))
}

pub async fn fetch_sender_proof_set(
    store_vault_server: &dyn StoreVaultClientInterface,
    ephemeral_key: PrivateKey,
) -> Result<SenderProofSet, StrategyError> {
    let encrypted_sender_proof_set = store_vault_server
        .get_snapshot(ephemeral_key, &DataType::SenderProofSet.to_topic())
        .await?
        .ok_or(StrategyError::SenderProofSetNotFound)?;
    let enc_sender = match DataType::SenderProofSet.rw_rights().write_rights {
        WriteRights::SingleAuthWrite => Some(ephemeral_key.to_public_key()),
        WriteRights::AuthWrite => Some(ephemeral_key.to_public_key()),
        WriteRights::SingleOpenWrite => None,
        WriteRights::OpenWrite => None,
    };
    let sender_proof_set =
        SenderProofSet::decrypt(ephemeral_key, enc_sender, &encrypted_sender_proof_set)?;
    Ok(sender_proof_set)
}

pub async fn fetch_user_data(
    store_vault_server: &dyn StoreVaultClientInterface,
    view_pair: ViewPair,
) -> Result<UserData, StrategyError> {
    let enc_sender = match DataType::UserData.rw_rights().write_rights {
        WriteRights::SingleAuthWrite => Some(view_pair.view.to_public_key()),
        WriteRights::AuthWrite => Some(view_pair.view.to_public_key()),
        WriteRights::SingleOpenWrite => None,
        WriteRights::OpenWrite => None,
    };
    let user_data = store_vault_server
        .get_snapshot(view_pair.view, &DataType::UserData.to_topic())
        .await?
        .map(|encrypted| UserData::decrypt(view_pair.view, enc_sender, &encrypted))
        .transpose()
        .map_err(|e| StrategyError::UserDataDecryptionError(e.to_string()))?
        .unwrap_or(UserData::new(view_pair.spend));
    Ok(user_data)
}

pub async fn fetch_single_data<T: BlsEncryption + Validation>(
    store_vault_server: &dyn StoreVaultClientInterface,
    view_priv: PrivateKey,
    data_type: DataType,
    digest: Bytes32,
) -> Result<(MetaData, T), StrategyError> {
    let data_with_meta = store_vault_server
        .get_data_batch(view_priv, &data_type.to_topic(), &[digest])
        .await?;
    if data_with_meta.len() != 1 {
        return Err(StrategyError::UnexpectedError(format!(
            "expected 1 data with digest {}, got {}",
            digest,
            data_with_meta.len()
        )));
    }
    let DataWithMetaData { meta, data } = data_with_meta.into_iter().next().unwrap();
    let enc_sender = match data_type.rw_rights().write_rights {
        WriteRights::SingleAuthWrite => Some(view_priv.to_public_key()),
        WriteRights::AuthWrite => Some(view_priv.to_public_key()),
        WriteRights::SingleOpenWrite => None,
        WriteRights::OpenWrite => None,
    };
    let (meta, data) = match T::decrypt(view_priv, enc_sender, &data) {
        Ok(data) => match data.validate() {
            Ok(_) => (meta, data),
            Err(e) => {
                return Err(StrategyError::ValidationError(format!(
                    "failed to validate {data_type}: {e}"
                )));
            }
        },
        Err(e) => {
            return Err(StrategyError::ValidationError(format!(
                "failed to decrypt {data_type}: {e}"
            )));
        }
    };
    Ok((meta, data))
}
