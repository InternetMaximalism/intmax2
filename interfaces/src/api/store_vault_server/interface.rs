use async_trait::async_trait;
use intmax2_zkp::{ethereum_types::u256::U256, utils::poseidon_hash_out::PoseidonHashOut};
use plonky2::{
    field::goldilocks_field::GoldilocksField,
    plonk::{config::PoseidonGoldilocksConfig, proof::ProofWithPublicInputs},
};
use serde::{Deserialize, Serialize};

use crate::{api::error::ServerError, data::meta_data::MetaData};

type F = GoldilocksField;
type C = PoseidonGoldilocksConfig;
const D: usize = 2;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum DataType {
    Deposit,
    Transfer,
    Withdrawal,
    Tx,
}

#[async_trait(?Send)]
pub trait StoreVaultClientInterface {
    async fn save_balance_proof(
        &self,
        pubkey: U256,
        proof: &ProofWithPublicInputs<F, C, D>,
    ) -> Result<(), ServerError>;

    async fn get_balance_proof(
        &self,
        pubkey: U256,
        block_number: u32,
        private_commitment: PoseidonHashOut,
    ) -> Result<Option<ProofWithPublicInputs<F, C, D>>, ServerError>;

    async fn save_data(
        &self,
        data_type: DataType,
        pubkey: U256,
        encrypted_data: &[u8],
    ) -> Result<(), ServerError>;

    async fn get_data(
        &self,
        data_type: DataType,
        uuid: &str,
    ) -> Result<Option<(MetaData, Vec<u8>)>, ServerError>;

    async fn get_data_all_after(
        &self,
        data_type: DataType,
        pubkey: U256,
        timestamp: u64,
    ) -> Result<Vec<(MetaData, Vec<u8>)>, ServerError>;

    async fn save_user_data(
        &self,
        pubkey: U256,
        encrypted_data: Vec<u8>,
    ) -> Result<(), ServerError>;

    async fn get_user_data(&self, pubkey: U256) -> Result<Option<Vec<u8>>, ServerError>;
}
