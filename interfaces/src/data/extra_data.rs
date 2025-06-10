use crate::utils::{address::keccak256, payment_id::PaymentId};
use intmax2_zkp::{
    common::salt::Salt,
    ethereum_types::{bytes32::Bytes32, u32limb_trait::U32LimbTrait},
    utils::poseidon_hash_out::PoseidonHashOut,
};
use serde::{Deserialize, Serialize};

/// Extra data for a transfer, which is binded to the salt in the transfer.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtraData {
    pub payment_id: Option<PaymentId>,
    pub description_hash: Option<Bytes32>,
    pub inner_salt: Option<Bytes32>,
}

impl ExtraData {
    pub fn to_salt(&self) -> Option<Salt> {
        // if all fields are None, return None
        if self.payment_id.is_none() && self.description_hash.is_none() && self.inner_salt.is_none()
        {
            return None;
        }
        // otherwise, create a salt from the fields
        let mut data: Vec<u32> = Vec::new();
        // Fields flag indicates which fields are present
        let mut fields_flag = 0;
        if let Some(payment_id) = &self.payment_id {
            fields_flag += 1;
            data.extend_from_slice(&payment_id.to_u32_vec());
        }
        if let Some(description_hash) = &self.description_hash {
            fields_flag += 2;
            data.extend_from_slice(&description_hash.to_u32_vec());
        }
        if let Some(inner_salt) = &self.inner_salt {
            fields_flag += 4;
            data.extend_from_slice(&inner_salt.to_u32_vec());
        }
        // Add the fields flag to the data for length padding
        data.push(fields_flag as u32);
        // Use Poseidon hash for ZKP compatibility in the future
        let hash = PoseidonHashOut::hash_inputs_u32(&data);
        Some(Salt(hash))
    }
}

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct FullExtraData {
    pub payment_id: Option<PaymentId>,
    pub description: Option<String>,
    pub description_salt: Option<Bytes32>,
    pub inner_salt: Option<Bytes32>,
}

impl FullExtraData {
    pub fn to_extra_data(&self) -> ExtraData {
        let description_hash = if let Some(desc) = &self.description {
            let mut desc_with_salt = desc.as_bytes().to_vec();
            if let Some(salt) = &self.description_salt {
                desc_with_salt.extend_from_slice(&salt.to_bytes_be());
            }
            let hash = keccak256(&desc_with_salt);
            Some(Bytes32::from_bytes_be(&hash).unwrap())
        } else {
            None
        };
        ExtraData {
            payment_id: self.payment_id,
            description_hash,
            inner_salt: self.inner_salt,
        }
    }
}
