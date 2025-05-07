use super::error::WithdrawalServerError;
use intmax2_interfaces::api::block_builder::interface::Fee;
use intmax2_zkp::ethereum_types::u256::U256;

pub fn parse_fee_str(fee: &str) -> Result<Vec<Fee>, WithdrawalServerError> {
    let mut fees = Vec::new();
    for fee_str in fee.split(',') {
        let fee_parts: Vec<&str> = fee_str.trim().split(':').map(str::trim).collect();
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

pub fn parse_optional_fee_str(
    fee: &Option<String>,
) -> Result<Option<Vec<Fee>>, WithdrawalServerError> {
    match fee {
        Some(f) => Ok(Some(parse_fee_str(f)?)),
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u256_from_u32(val: u32) -> U256 {
        U256::from(val)
    }

    #[test]
    fn test_valid_single_fee() {
        let result = parse_fee_str("1:42").unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].token_index, 1);
        assert_eq!(result[0].amount, u256_from_u32(42));
    }

    #[test]
    fn test_valid_single_fee_with_whitespaces() {
        let result = parse_fee_str("   1:42    ").unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].token_index, 1);
        assert_eq!(result[0].amount, u256_from_u32(42));
    }

    #[test]
    fn test_invalid_single_fee_with_whitespaces() {
        let result = parse_fee_str(" 1:4 2 ");
        assert!(matches!(result, Err(WithdrawalServerError::ParseError(_))));
    }

    #[test]
    fn test_valid_multiple_fees() {
        let result = parse_fee_str("1:100,2:200").unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(
            result[0],
            Fee {
                token_index: 1,
                amount: u256_from_u32(100)
            }
        );
        assert_eq!(
            result[1],
            Fee {
                token_index: 2,
                amount: u256_from_u32(200)
            }
        );
    }

    #[test]
    fn test_valid_multiple_fees_with_whitespaces() {
        let result = parse_fee_str(" 1 : 100 , 2 : 200 ").unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(
            result[0],
            Fee {
                token_index: 1,
                amount: u256_from_u32(100)
            }
        );
        assert_eq!(
            result[1],
            Fee {
                token_index: 2,
                amount: u256_from_u32(200)
            }
        );
    }

    #[test]
    fn test_three_multiple_fees() {
        let result = parse_fee_str("1:100,2:200,3:300").unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(
            result[0],
            Fee {
                token_index: 1,
                amount: u256_from_u32(100)
            }
        );
        assert_eq!(
            result[1],
            Fee {
                token_index: 2,
                amount: u256_from_u32(200)
            }
        );
        assert_eq!(
            result[2],
            Fee {
                token_index: 3,
                amount: u256_from_u32(300)
            }
        );
    }

    #[test]
    fn test_empty_input() {
        let result = parse_fee_str("");
        assert!(matches!(result, Err(WithdrawalServerError::ParseError(_))));
    }

    #[test]
    fn test_missing_fee_amount() {
        let result = parse_fee_str("1:");
        assert!(matches!(result, Err(WithdrawalServerError::ParseError(_))));
    }

    #[test]
    fn test_missing_token_index() {
        let result = parse_fee_str(":100");
        assert!(matches!(result, Err(WithdrawalServerError::ParseError(_))));
    }

    #[test]
    fn test_missing_colon_separator() {
        let result = parse_fee_str("1100");
        assert!(matches!(result, Err(WithdrawalServerError::ParseError(_))));
    }

    #[test]
    fn test_invalid_format_missing_colon() {
        let input = "1-1000";
        let err = parse_fee_str(input).unwrap_err();
        match err {
            WithdrawalServerError::ParseError(msg) => {
                assert!(msg.contains("Invalid fee format"))
            }
            _ => panic!("Unexpected error variant"),
        }
    }

    #[test]
    fn test_extra_colon_separator() {
        let result = parse_fee_str("1:100:extra");
        assert!(matches!(result, Err(WithdrawalServerError::ParseError(_))));
    }

    #[test]
    fn test_non_numeric_token_index() {
        let result = parse_fee_str("abc:100");
        assert!(matches!(result, Err(WithdrawalServerError::ParseError(_))));
    }

    #[test]
    fn test_non_numeric_fee_amount() {
        let result = parse_fee_str("1:abc");
        assert!(matches!(result, Err(WithdrawalServerError::ParseError(_))));
    }

    #[test]
    fn test_u32_max_token_index() {
        let max = u32::MAX;
        let result = parse_fee_str(&format!("{}:100", max)).unwrap();
        assert_eq!(result[0].token_index, max);
        assert_eq!(result[0].amount, u256_from_u32(100));
    }

    #[test]
    fn test_u32_overflow_token_index() {
        let result = parse_fee_str("4294967296:100"); // u32::MAX + 1
        assert!(matches!(result, Err(WithdrawalServerError::ParseError(_))));
    }

    #[test]
    fn test_u256_large_decimal_value() {
        // The largest U256: 2^256 - 1 = 115792089237316195423570985008687907853269984665640564039457584007913129639935
        let max_u256 =
            "115792089237316195423570985008687907853269984665640564039457584007913129639935";
        let result = parse_fee_str(&format!("0:{}", max_u256)).unwrap();
        assert_eq!(result[0].token_index, 0);
        assert_eq!(format!("{}", result[0].amount), max_u256);
    }

    #[test]
    fn test_u256_too_large() {
        // 2^256 is too big
        let too_big =
            "115792089237316195423570985008687907853269984665640564039457584007913129639936";
        let result = parse_fee_str(&format!("0:{}", too_big));
        assert!(matches!(result, Err(WithdrawalServerError::ParseError(_))));
    }

    #[test]
    fn test_trailing_comma() {
        let result = parse_fee_str("1:100,");
        assert!(matches!(result, Err(WithdrawalServerError::ParseError(_))));
    }

    #[test]
    fn test_leading_comma() {
        let result = parse_fee_str(",1:100");
        assert!(matches!(result, Err(WithdrawalServerError::ParseError(_))));
    }

    #[test]
    fn test_valid_hex_fee() {
        let result = parse_fee_str("3:0x64").unwrap();
        assert_eq!(
            result[0],
            Fee {
                token_index: 3,
                amount: u256_from_u32(0x64)
            }
        );
    }

    #[test]
    fn test_invalid_hex_fee() {
        let result = parse_fee_str("3:0xZZ");
        assert!(matches!(result, Err(WithdrawalServerError::ParseError(_))));
    }
}
