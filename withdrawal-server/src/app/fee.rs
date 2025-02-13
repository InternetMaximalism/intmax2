use std::collections::HashMap;

use intmax2_interfaces::api::block_builder::interface::Fee;
use intmax2_zkp::ethereum_types::u256::U256;

use super::error::WithdrawalServerError;

pub fn parse_fee_str(fee: &str) -> Result<HashMap<u32, U256>, WithdrawalServerError> {
    let mut fee_map = HashMap::new();
    for fee_str in fee.split(',') {
        let fee_parts: Vec<&str> = fee_str.split(':').collect();
        if fee_parts.len() != 2 {
            return Err(WithdrawalServerError::ParseError(
                "Invalid fee format: should be token_index:fee_amount".to_string(),
            ));
        }
        let token_index = fee_parts[0].parse::<u32>().map_err(|e| {
            WithdrawalServerError::ParseError(format!("Failed to parse token index: {}", e))
        })?;
        let fee_amount: U256 = fee_parts[1].parse().map_err(|e| {
            WithdrawalServerError::ParseError(format!("Failed to convert fee amount: {}", e))
        })?;
        fee_map.insert(token_index, fee_amount);
    }
    Ok(fee_map)
}

pub fn convert_fee_vec(fee: &Option<HashMap<u32, U256>>) -> Option<Vec<Fee>> {
    fee.as_ref().map(|fee| {
        fee.iter()
            .map(|(token_index, amount)| Fee {
                token_index: *token_index,
                amount: *amount,
            })
            .collect()
    })
}
