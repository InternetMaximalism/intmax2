use plonky2::{
    field::{extension::Extendable, goldilocks_field::GoldilocksField},
    hash::hash_types::RichField,
    plonk::config::{GenericConfig, PoseidonGoldilocksConfig},
};

use crate::{
    client::error::ClientError,
    external_api::{
        block_validity_prover::interface::BlockValidityInterface,
        store_vault_server::interface::StoreVaultInterface,
    },
};

use intmax2_zkp::{
    common::signature::key_set::KeySet,
    mock::data::{meta_data::MetaData, tx_data::TxData},
};

type F = GoldilocksField;
type C = PoseidonGoldilocksConfig;
const D: usize = 2;

#[derive(Debug, Clone)]
pub struct TxInfo<F, C, const D: usize>
where
    F: RichField + Extendable<D>,
    C: GenericConfig<D, F = F>,
{
    pub settled: Vec<(MetaData, TxData<F, C, D>)>,
    pub pending: Vec<MetaData>,
    pub rejected: Vec<MetaData>,
}

pub async fn fetch_tx_info<S: StoreVaultInterface, V: BlockValidityInterface>(
    store_vault_server: &S,
    validity_prover: &V,
    key: KeySet,
    tx_lpt: u64,
    tx_timeout: u64,
) -> Result<TxInfo<F, C, D>, ClientError> {
    let mut settled = Vec::new();
    let mut pending = Vec::new();
    let mut rejected = Vec::new();

    let encrypted_data = store_vault_server
        .get_tx_data_all_after(key.pubkey, tx_lpt)
        .await?;
    for (meta, encrypted_data) in encrypted_data {
        match TxData::decrypt(&encrypted_data, key) {
            Ok(tx_data) => {
                let tx_tree_root = tx_data.common.tx_tree_root;
                let block_number = validity_prover
                    .get_block_number_by_tx_tree_root(tx_tree_root)
                    .await?;
                if let Some(block_number) = block_number {
                    // set block number
                    let mut meta = meta;
                    meta.block_number = Some(block_number);
                    settled.push((meta, tx_data));
                } else {
                    if meta.timestamp + tx_timeout < chrono::Utc::now().timestamp() as u64 {
                        // timeout
                        log::error!("Tx {} is timeouted", meta.uuid);
                        rejected.push(meta);
                    } else {
                        // pending
                        log::info!("Tx {} is pending", meta.uuid);
                        pending.push(meta);
                    }
                }
            }
            Err(e) => {
                log::error!("failed to decrypt tx data: {}", e);
                rejected.push(meta);
            }
        };
    }

    // sort by block number
    settled.sort_by_key(|(meta, _)| meta.block_number.unwrap());

    Ok(TxInfo {
        settled,
        pending,
        rejected,
    })
}
