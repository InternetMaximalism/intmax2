use std::{fmt, str::FromStr};

use intmax2_zkp::ethereum_types::u256::U256;
use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};

#[derive(Default, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Fee {
    pub token_index: u32,
    pub amount: U256,
}

#[derive(Debug, thiserror::Error)]
pub enum FeeListError {
    #[error("Invalid fee format at position {position}: should be token_index:fee_amount")]
    InvalidFormat { position: usize },
    #[error("Failed to parse token index at position {position}: {message}")]
    TokenIndexParseError { position: usize, message: String },
    #[error("Failed to parse fee amount at position {position}: {message}")]
    AmountParseError { position: usize, message: String },
}

#[derive(Clone, Debug, SerializeDisplay, DeserializeFromStr)]
pub struct FeeList(pub Vec<Fee>);

impl fmt::Display for FeeList {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let fees: Vec<String> = self
            .0
            .iter()
            .map(|fee| format!("{}:{}", fee.token_index, fee.amount))
            .collect();
        write!(f, "{}", fees.join(","))
    }
}

impl FromStr for FeeList {
    type Err = FeeListError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            return Ok(FeeList(Vec::new()));
        }

        let mut fees = Vec::new();

        for (position, fee_str) in trimmed.split(',').enumerate() {
            let fee_str = fee_str.trim();
            let parts: Vec<&str> = fee_str.split(':').map(str::trim).collect();

            if parts.len() != 2 {
                return Err(FeeListError::InvalidFormat { position });
            }

            let token_index =
                parts[0]
                    .parse::<u32>()
                    .map_err(|e| FeeListError::TokenIndexParseError {
                        position,
                        message: e.to_string(),
                    })?;

            let amount = parts[1]
                .parse::<U256>()
                .map_err(|e| FeeListError::AmountParseError {
                    position,
                    message: e.to_string(),
                })?;

            fees.push(Fee {
                token_index,
                amount,
            });
        }

        Ok(FeeList(fees))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_fee_default() {
        let fee = Fee::default();
        assert_eq!(fee.token_index, 0);
        assert_eq!(fee.amount, U256::from(0));
    }

    #[test]
    fn test_fee_clone_and_debug() {
        let fee = Fee {
            token_index: 1,
            amount: U256::from(100),
        };
        let cloned = fee.clone();
        assert_eq!(fee, cloned);

        // Debug trait test
        let debug_str = format!("{fee:?}");
        assert!(debug_str.contains("token_index: 1"));
        assert!(debug_str.contains("amount"));
    }

    #[test]
    fn test_fee_list_display_single_fee() {
        let fee_list = FeeList(vec![Fee {
            token_index: 1,
            amount: U256::from(100),
        }]);

        assert_eq!(fee_list.to_string(), "1:100");
    }

    #[test]
    fn test_fee_list_display_multiple_fees() {
        let fee_list = FeeList(vec![
            Fee {
                token_index: 1,
                amount: U256::from(100),
            },
            Fee {
                token_index: 2,
                amount: U256::from(200),
            },
            Fee {
                token_index: 3,
                amount: U256::from(300),
            },
        ]);

        assert_eq!(fee_list.to_string(), "1:100,2:200,3:300");
    }

    #[test]
    fn test_fee_list_display_empty() {
        let fee_list = FeeList(vec![]);
        assert_eq!(fee_list.to_string(), "");
    }

    #[test]
    fn test_fee_list_from_str_single_fee() {
        let fee_list = FeeList::from_str("1:100").unwrap();
        assert_eq!(fee_list.0.len(), 1);
        assert_eq!(fee_list.0[0].token_index, 1);
        assert_eq!(fee_list.0[0].amount, U256::from(100));
    }

    #[test]
    fn test_fee_list_from_str_multiple_fees() {
        let fee_list = FeeList::from_str("1:100,2:200,3:300").unwrap();
        assert_eq!(fee_list.0.len(), 3);

        assert_eq!(fee_list.0[0].token_index, 1);
        assert_eq!(fee_list.0[0].amount, U256::from(100));

        assert_eq!(fee_list.0[1].token_index, 2);
        assert_eq!(fee_list.0[1].amount, U256::from(200));

        assert_eq!(fee_list.0[2].token_index, 3);
        assert_eq!(fee_list.0[2].amount, U256::from(300));
    }

    #[test]
    fn test_fee_list_from_str_with_whitespace() {
        let fee_list = FeeList::from_str("  1 : 100 , 2 : 200  ").unwrap();
        assert_eq!(fee_list.0.len(), 2);
        assert_eq!(fee_list.0[0].token_index, 1);
        assert_eq!(fee_list.0[0].amount, U256::from(100));
        assert_eq!(fee_list.0[1].token_index, 2);
        assert_eq!(fee_list.0[1].amount, U256::from(200));
    }

    #[test]
    fn test_fee_list_from_str_large_numbers() {
        let large_amount = "1000000000000000000000000000000";
        let fee_list = FeeList::from_str(&format!("1:{large_amount}")).unwrap();
        assert_eq!(fee_list.0.len(), 1);
        assert_eq!(fee_list.0[0].token_index, 1);
        assert_eq!(fee_list.0[0].amount, U256::from_str(large_amount).unwrap());
    }

    #[test]
    fn test_fee_list_from_str_invalid_format_no_colon() {
        let result = FeeList::from_str("1-100");
        assert!(matches!(
            result,
            Err(FeeListError::InvalidFormat { position: 0 })
        ));
    }

    #[test]
    fn test_fee_list_from_str_invalid_format_multiple_colons() {
        let result = FeeList::from_str("1:100:200");
        assert!(matches!(
            result,
            Err(FeeListError::InvalidFormat { position: 0 })
        ));
    }

    #[test]
    fn test_fee_list_from_str_invalid_token_index() {
        let result = FeeList::from_str("abc:100");
        assert!(matches!(
            result,
            Err(FeeListError::TokenIndexParseError { position: 0, .. })
        ));
    }

    #[test]
    fn test_fee_list_from_str_negative_token_index() {
        let result = FeeList::from_str("-1:100");
        assert!(matches!(
            result,
            Err(FeeListError::TokenIndexParseError { position: 0, .. })
        ));
    }

    #[test]
    fn test_fee_list_from_str_invalid_amount() {
        let result = FeeList::from_str("1:abc");
        assert!(matches!(
            result,
            Err(FeeListError::AmountParseError { position: 0, .. })
        ));
    }

    #[test]
    fn test_fee_list_from_str_negative_amount() {
        let result = FeeList::from_str("1:-100");
        assert!(matches!(
            result,
            Err(FeeListError::AmountParseError { position: 0, .. })
        ));
    }

    #[test]
    fn test_fee_list_from_str_error_position() {
        let result = FeeList::from_str("1:100,abc:200,3:300");
        assert!(matches!(
            result,
            Err(FeeListError::TokenIndexParseError { position: 1, .. })
        ));
    }

    #[test]
    fn test_fee_list_from_str_error_position_amount() {
        let result = FeeList::from_str("1:100,2:abc,3:300");
        assert!(matches!(
            result,
            Err(FeeListError::AmountParseError { position: 1, .. })
        ));
    }

    #[test]
    fn test_fee_list_round_trip() {
        let original = FeeList(vec![
            Fee {
                token_index: 1,
                amount: U256::from(100),
            },
            Fee {
                token_index: 2,
                amount: U256::from(200),
            },
        ]);

        let string_repr = original.to_string();
        let parsed = FeeList::from_str(&string_repr).unwrap();

        assert_eq!(original.0.len(), parsed.0.len());
        for (orig, parsed) in original.0.iter().zip(parsed.0.iter()) {
            assert_eq!(orig.token_index, parsed.token_index);
            assert_eq!(orig.amount, parsed.amount);
        }
    }

    #[test]
    fn test_fee_list_error_display() {
        let error = FeeListError::InvalidFormat { position: 2 };
        let error_str = error.to_string();
        assert!(error_str.contains("Invalid fee format at position 2"));
        assert!(error_str.contains("should be token_index:fee_amount"));

        let error = FeeListError::TokenIndexParseError {
            position: 1,
            message: "invalid digit found in string".to_string(),
        };
        let error_str = error.to_string();
        assert!(error_str.contains("Failed to parse token index at position 1"));
        assert!(error_str.contains("invalid digit found in string"));

        let error = FeeListError::AmountParseError {
            position: 0,
            message: "invalid character".to_string(),
        };
        let error_str = error.to_string();
        assert!(error_str.contains("Failed to parse fee amount at position 0"));
        assert!(error_str.contains("invalid character"));
    }

    #[test]
    fn test_fee_list_zero_values() {
        let fee_list = FeeList::from_str("0:0").unwrap();
        assert_eq!(fee_list.0.len(), 1);
        assert_eq!(fee_list.0[0].token_index, 0);
        assert_eq!(fee_list.0[0].amount, U256::from(0));
    }

    #[test]
    fn test_fee_list_max_u32_token_index() {
        let max_u32 = u32::MAX;
        let fee_list = FeeList::from_str(&format!("{max_u32}:100")).unwrap();
        assert_eq!(fee_list.0.len(), 1);
        assert_eq!(fee_list.0[0].token_index, max_u32);
        assert_eq!(fee_list.0[0].amount, U256::from(100));
    }

    #[test]
    fn test_fee_list_overflow_token_index() {
        let overflow_value = (u32::MAX as u64) + 1;
        let result = FeeList::from_str(&format!("{overflow_value}:100"));
        assert!(matches!(
            result,
            Err(FeeListError::TokenIndexParseError { position: 0, .. })
        ));
    }
}
