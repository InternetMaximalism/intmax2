use intmax2_interfaces::{
    data::{
        deposit_data::DepositData, extra_data::FullExtraData, transfer_data::TransferData,
        tx_data::TxData,
    },
    utils::{
        address::{AddressType, IntmaxAddress},
        fee::Fee,
        key::PrivateKey,
        payment_id::PaymentId,
        random::default_rng,
    },
};
use intmax2_zkp::{
    common::{
        generic_address::GenericAddress, transfer::Transfer, tx::Tx,
        witness::spent_witness::SpentWitness,
    },
    ethereum_types::{
        address::Address, bytes32::Bytes32, u256::U256, u32limb_trait::U32LimbTrait as _,
    },
};
use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use std::{fmt, str::FromStr};

use crate::client::sync::utils::generate_salt;

#[derive(Debug, Clone, thiserror::Error)]
pub enum GenericAddressError {
    #[error("Invalid address {0}")]
    InvalidAddress(String),

    #[error("Invalid Intmax address: {0}")]
    InvalidIntmaxAddress(String),
}

#[derive(Debug, Clone, SerializeDisplay, DeserializeFromStr)]
pub enum GenericRecipient {
    IntmaxAddress(IntmaxAddress),
    Address(Address),
}

impl fmt::Display for GenericRecipient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GenericRecipient::IntmaxAddress(intmax_address) => write!(f, "{intmax_address}"),
            GenericRecipient::Address(address) => write!(f, "{address}"),
        }
    }
}

impl FromStr for GenericRecipient {
    type Err = GenericAddressError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.starts_with("0x") {
            // Assuming the string is an Ethereum address
            let address = Address::from_str(s)
                .map_err(|e| GenericAddressError::InvalidAddress(e.to_string()))?;
            Ok(GenericRecipient::Address(address))
        } else {
            // Assuming the string is an Intmax address
            let intmax_address = IntmaxAddress::from_str(s)
                .map_err(|e| GenericAddressError::InvalidIntmaxAddress(e.to_string()))?;
            Ok(GenericRecipient::IntmaxAddress(intmax_address))
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferRequest {
    pub recipient: GenericRecipient,
    pub token_index: u32,
    pub amount: U256,
    pub description: Option<String>,
}

impl TransferRequest {
    pub fn to_transfer_and_full_extra_data(&self) -> (Transfer, FullExtraData) {
        let (recipient, payment_id): (GenericAddress, Option<PaymentId>) = match self.recipient {
            GenericRecipient::IntmaxAddress(intmax_address) => {
                let payment_id = match intmax_address.addr_type {
                    AddressType::Standard => None,
                    AddressType::Integrated(payment_id) => Some(payment_id),
                };
                (intmax_address.public_spend.0.into(), payment_id)
            }
            GenericRecipient::Address(address) => (address.into(), None),
        };
        let mut rng = default_rng();
        let description_salt = if self.description.is_some() {
            Some(Bytes32::rand(&mut rng))
        } else {
            None
        };
        let inner_salt = if payment_id.is_some() || self.description.is_some() {
            Some(Bytes32::rand(&mut rng))
        } else {
            None
        };
        let full_extra_data = FullExtraData {
            payment_id,
            description: self.description.clone(),
            description_salt,
            inner_salt,
        };
        // full extra data is binded to the salt
        // if the salt is not provided, generate a new one
        let salt = full_extra_data
            .to_extra_data()
            .to_salt()
            .unwrap_or(generate_salt());
        let transfer = Transfer {
            recipient,
            amount: self.amount,
            token_index: self.token_index,
            salt,
        };
        (transfer, full_extra_data)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentMemoEntry {
    pub transfer_index: u32,
    pub topic: String,
    pub memo: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TxRequestMemo {
    pub request_id: String,
    pub is_registration_block: bool,
    pub tx: Tx,
    pub transfers: Vec<Transfer>,
    pub recipients: Vec<GenericRecipient>,
    pub full_extra_data: Vec<FullExtraData>,
    pub spent_witness: SpentWitness,
    pub sender_proof_set_ephemeral_key: PrivateKey,
    pub payment_memos: Vec<PaymentMemoEntry>,
    pub fee_index: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DepositResult {
    pub deposit_data: DepositData,
    pub deposit_digest: Bytes32,
    pub backup_csv: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferFeeQuote {
    pub beneficiary: IntmaxAddress,
    pub fee: Option<Fee>,
    pub collateral_fee: Option<Fee>,
    pub block_builder_address: Address,
    pub is_registration_block: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeeQuote {
    pub beneficiary: IntmaxAddress,
    pub fee: Option<Fee>,
    pub collateral_fee: Option<Fee>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TxResult {
    pub tx_tree_root: Bytes32,
    pub tx_digest: Bytes32,
    pub tx_data: TxData,
    pub transfer_data_vec: Vec<TransferData>,
    pub backup_csv: String,
}
