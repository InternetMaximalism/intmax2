use ethers::{types::Address, utils::hex::ToHexExt};
use sqlx::{Pool, Postgres};

use super::error::SettingConsistencyError;

// CREATE TABLE IF NOT EXISTS settings (
//     singleton_key BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (singleton_key),
//     rollup_contract_address VARCHAR(42) NOT NULL,
//     liquidity_contract_address VARCHAR(42) NOT NULL
// );

pub struct SettingConsistency {
    pub pool: Pool<Postgres>,
}

impl SettingConsistency {
    pub fn new(pool: Pool<Postgres>) -> Self {
        SettingConsistency { pool }
    }

    pub async fn check_consistency(
        &self,
        rollup_contract_address: Address,
        liquidity_contract_address: Address,
    ) -> Result<(), SettingConsistencyError> {
        // Convert addresses to checksum format strings for consistent comparison
        let rollup_addr_str = rollup_contract_address.encode_hex_with_prefix();
        let liquidity_addr_str = liquidity_contract_address.encode_hex_with_prefix();
        // Try to select existing settings
        let existing = sqlx::query!(
            r#"
            SELECT rollup_contract_address, liquidity_contract_address 
            FROM settings 
            WHERE singleton_key = true
            "#
        )
        .fetch_optional(&self.pool)
        .await?;

        match existing {
            // If no settings exist, insert new settings
            None => {
                sqlx::query!(
                    r#"
                    INSERT INTO settings 
                    (rollup_contract_address, liquidity_contract_address) 
                    VALUES ($1, $2)
                    "#,
                    rollup_addr_str,
                    liquidity_addr_str
                )
                .execute(&self.pool)
                .await?;
                Ok(())
            }
            // If settings exist, compare with provided addresses
            Some(record) => {
                if record.rollup_contract_address != rollup_addr_str {
                    return Err(SettingConsistencyError::MismatchedSetting(format!(
                        "Rollup contract address mismatch. Expected: {}, Got: {}",
                        record.rollup_contract_address, rollup_addr_str
                    )));
                }
                if record.liquidity_contract_address != liquidity_addr_str {
                    return Err(SettingConsistencyError::MismatchedSetting(format!(
                        "Liquidity contract address mismatch. Expected: {}, Got: {}",
                        record.liquidity_contract_address, liquidity_addr_str
                    )));
                }
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_setting_consistency() {
        let pool = Pool::connect("postgres://postgres:password@localhost/validity_prover")
            .await
            .unwrap();

        let setting_consistency = SettingConsistency::new(pool);

        // Test with dummy addresses
        let rollup_address = "0x1234567890abcdef1234567890abcdef12345678"
            .parse()
            .unwrap();
        let liquidity_address = "0xabcdefabcdefabcdefabcdefabcdefabcdefabcd"
            .parse()
            .unwrap();

        // Check consistency
        let result = setting_consistency
            .check_consistency(rollup_address, liquidity_address)
            .await;

        assert!(result.is_ok());

        // Check again with the same addresses
        let result = setting_consistency
            .check_consistency(rollup_address, liquidity_address)
            .await;
        assert!(result.is_ok());
    }
}
