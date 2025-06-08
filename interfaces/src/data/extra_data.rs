use intmax2_zkp::{common::salt::Salt, ethereum_types::bytes32::Bytes32};

use crate::utils::payment_id::PaymentId;

/// Extra data for a transfer
pub struct ExtraData {
    pub payment_id: Option<PaymentId>,
    pub description_hash: Option<Bytes32>,
    pub inner_salt: Option<Salt>,
}

impl ExtraData {
    pub fn to_salt(&self) -> Option<Salt> {
        // if all fields are None, return None
        if self.payment_id.is_none() && self.description_hash.is_none() && self.inner_salt.is_none()
        {
            return None;
        }
        // otherwise, create a salt from the fields
        let mut data = Vec::new();
        if let Some(payment_id) = &self.payment_id {
            data.extend_from_slice(&payment_id.0);
        }
        if let Some(description_hash) = &self.description_hash {
            data.extend_from_slice(description_hash.as_ref());
        }

        todo!()
    }
}
