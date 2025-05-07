use alloy::{hex::ToHexExt, primitives::Address};
use sqlx::{Pool, Postgres};

use super::error::SettingConsistencyError;

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
