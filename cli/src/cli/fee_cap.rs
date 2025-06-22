use intmax2_interfaces::utils::fee::{Fee, FeeList};
use intmax2_zkp::ethereum_types::u256::U256;

#[derive(Debug, Clone, thiserror::Error)]
pub enum FeeCapError {
    #[error("Fee cap not set for token index {token_index}")]
    FeeCapNotSetForTokenIndex { token_index: u32 },

    #[error(
        "Fee amount {amount} exceeds the maximum allowed fee cap {max_amount} for token index {token_index}"
    )]
    FeeExceedsCap {
        token_index: u32,
        amount: U256,
        max_amount: U256,
    },
}

pub fn validate_fee_cap(fee: &Fee, fee_caps: &Option<FeeList>) -> Result<(), FeeCapError> {
    if let Some(fee_caps) = fee_caps {
        if let Some(fee_cap) = fee_caps
            .0
            .iter()
            .find(|fee_cap| fee_cap.token_index == fee.token_index)
        {
            if fee.amount > fee_cap.amount {
                return Err(FeeCapError::FeeExceedsCap {
                    token_index: fee.token_index,
                    amount: fee.amount,
                    max_amount: fee_cap.amount,
                });
            }
        } else {
            return Err(FeeCapError::FeeCapNotSetForTokenIndex {
                token_index: fee.token_index,
            });
        }
    }
    Ok(())
}
