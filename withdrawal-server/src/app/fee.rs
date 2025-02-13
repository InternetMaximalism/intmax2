use super::error::WithdrawalServerError;
use intmax2_interfaces::api::block_builder::interface::Fee;
use intmax2_zkp::ethereum_types::u256::U256;

pub fn parse_fee_str(fee: &str) -> Result<Vec<Fee>, WithdrawalServerError> {
    let mut fees = Vec::new();
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
        let amount: U256 = fee_parts[1].parse().map_err(|e| {
            WithdrawalServerError::ParseError(format!("Failed to convert fee amount: {}", e))
        })?;
        fees.push(Fee {
            token_index,
            amount,
        });
    }
    Ok(fees)
}
