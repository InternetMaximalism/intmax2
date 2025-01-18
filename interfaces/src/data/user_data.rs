use hashbrown::HashMap;
use serde::{Deserialize, Serialize};

use intmax2_zkp::{
    circuits::balance::balance_processor::get_prev_balance_pis,
    common::{
        private_state::{FullPrivateState, PrivateState},
        trees::asset_tree::AssetLeaf,
    },
    ethereum_types::u256::U256,
    utils::poseidon_hash_out::PoseidonHashOut,
};

use super::{
    deposit_data::DepositData, encryption::Encryption, error::DataError, meta_data::MetaData,
    proof_compression::CompressedBalanceProof, transfer_data::TransferData, tx_data::TxData,
};

type Result<T> = std::result::Result<T, DataError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserData {
    pub pubkey: U256,
    pub full_private_state: FullPrivateState,
    pub balance_proof: Option<CompressedBalanceProof>,
    pub deposit_status: ProcessStatus,
    pub transfer_status: ProcessStatus,
    pub tx_status: ProcessStatus,
    pub withdrawal_status: ProcessStatus,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessStatus {
    // Last processed meta data
    pub last_processed_meta_data: Option<MetaData>,
    pub processed_uuids: Vec<String>,
    pub pending_uuids: Vec<String>,
}

impl ProcessStatus {
    pub fn process(&mut self, meta: MetaData) {
        self.last_processed_meta_data = Some(meta.clone());
        self.pending_uuids.retain(|uuid| uuid != &meta.uuid);
        self.processed_uuids.push(meta.uuid);
    }
}

impl UserData {
    pub fn new(pubkey: U256) -> Self {
        Self {
            pubkey,
            full_private_state: FullPrivateState::new(),

            balance_proof: None,

            deposit_status: ProcessStatus::default(),
            transfer_status: ProcessStatus::default(),
            tx_status: ProcessStatus::default(),
            withdrawal_status: ProcessStatus::default(),
        }
    }

    pub fn block_number(&self) -> Result<u32> {
        let balance_proof = self
            .balance_proof
            .as_ref()
            .map(|bp| bp.decompress())
            .transpose()?;
        let balance_pis = get_prev_balance_pis(self.pubkey, &balance_proof);
        Ok(balance_pis.public_state.block_number)
    }

    pub fn private_state(&self) -> PrivateState {
        self.full_private_state.to_private_state()
    }

    pub fn private_commitment(&self) -> PoseidonHashOut {
        self.full_private_state.to_private_state().commitment()
    }

    pub fn balances(&self) -> Balances {
        let leaves = self
            .full_private_state
            .asset_tree
            .leaves()
            .into_iter()
            .map(|(index, leaf)| (index as u32, leaf))
            .collect();
        Balances(leaves)
    }
}

impl Encryption for UserData {}

/// Token index -> AssetLeaf
pub struct Balances(pub HashMap<u32, AssetLeaf>);

impl Balances {
    pub fn is_insufficient(&self) -> bool {
        let mut is_insufficient = false;
        for (_token_index, asset_leaf) in self.0.iter() {
            is_insufficient = is_insufficient || asset_leaf.is_insufficient;
        }
        is_insufficient
    }

    /// Update the balance with the deposit data
    pub fn add_deposit(&mut self, deposit_data: &DepositData) {
        let token_index = deposit_data.token_index.unwrap();
        let prev_asset_leaf = self.0.get(&token_index).cloned().unwrap_or_default();
        let new_asset_leaf = prev_asset_leaf.add(deposit_data.amount);
        self.0.insert(token_index, new_asset_leaf);
    }

    /// Update the balance with the transfer data
    pub fn add_transfer(&mut self, transfer_data: &TransferData) {
        let token_index = transfer_data.transfer.token_index;
        let prev_asset_leaf = self.0.get(&token_index).cloned().unwrap_or_default();
        let new_asset_leaf = prev_asset_leaf.add(transfer_data.transfer.amount);
        self.0.insert(token_index, new_asset_leaf);
    }

    /// Update the balance with the tx data
    /// Returns whether the tx will case insufficient balance
    pub fn sub_tx(&mut self, tx_data: &TxData) -> bool {
        let transfers = &tx_data.spent_witness.transfers;
        let mut is_insufficient = false;
        for transfer in transfers.iter() {
            let token_index = transfer.token_index;
            let prev_asset_leaf = self.0.get(&token_index).cloned().unwrap_or_default();
            let new_asset_leaf = prev_asset_leaf.sub(transfer.amount);
            is_insufficient = is_insufficient || new_asset_leaf.is_insufficient;
            self.0.insert(token_index, new_asset_leaf);
        }
        is_insufficient
    }
}
