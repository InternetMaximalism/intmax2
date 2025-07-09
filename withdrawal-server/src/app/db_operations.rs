use intmax2_interfaces::api::{
    store_vault_server::types::CursorOrder,
    withdrawal_server::{
        interface::{ClaimInfo, ContractWithdrawal, WithdrawalInfo},
        types::{TimestampCursor, TimestampCursorResponse},
    },
};
use intmax2_zkp::{
    common::{claim::Claim, transfer::Transfer},
    ethereum_types::{address::Address, bytes32::Bytes32, u256::U256, u32limb_trait::U32LimbTrait},
};
use server_common::db::DbPool;

use super::{
    error::WithdrawalServerError,
    status::{SqlClaimStatus, SqlWithdrawalStatus},
};

const PG_UNIQUE_VIOLATION_CODE: &str = "23505"; // PostgreSQL error code for unique_violation

pub struct DbOperations<'a> {
    pool: &'a DbPool,
}

impl<'a> DbOperations<'a> {
    pub fn new(pool: &'a DbPool) -> Self {
        Self { pool }
    }

    pub async fn get_withdrawal_info(
        &self,
        pubkey: U256,
        cursor: TimestampCursor,
    ) -> Result<(Vec<WithdrawalInfo>, TimestampCursorResponse), WithdrawalServerError> {
        let pubkey_str = pubkey.to_hex();
        let actual_limit = cursor.limit.unwrap_or(100) as i64;

        let withdrawal_infos: Vec<WithdrawalInfo> = match cursor.order {
            CursorOrder::Asc => {
                let cursor_timestamp = cursor.cursor.unwrap_or(0) as i64;
                sqlx::query!(
                    r#"
              SELECT 
                  status as "status: SqlWithdrawalStatus",
                  contract_withdrawal,
                  l1_tx_hash,
                  created_at
              FROM withdrawals
              WHERE pubkey = $1
              AND EXTRACT(EPOCH FROM created_at)::bigint > $2
              ORDER BY created_at ASC
              LIMIT $3
              "#,
                    pubkey_str,
                    cursor_timestamp,
                    actual_limit + 1
                )
                .fetch_all(self.pool)
                .await?
                .into_iter()
                .map(|record| {
                    // Convert the record to WithdrawalInfo
                    let contract_withdrawal: ContractWithdrawal =
                        serde_json::from_value(record.contract_withdrawal).map_err(|e| {
                            WithdrawalServerError::SerializationError(e.to_string())
                        })?;
                    Ok(WithdrawalInfo {
                        status: record.status.into(),
                        contract_withdrawal,
                        l1_tx_hash: record.l1_tx_hash.map(|h| Bytes32::from_hex(&h).unwrap()),
                        requested_at: record.created_at.timestamp() as u64,
                    })
                })
                .collect::<Result<Vec<_>, WithdrawalServerError>>()?
            }
            CursorOrder::Desc => {
                let cursor_timestamp = cursor.cursor.unwrap_or(i64::MAX as u64) as i64;
                sqlx::query!(
                    r#"
              SELECT 
                  status as "status: SqlWithdrawalStatus",
                  contract_withdrawal,
                  l1_tx_hash,
                  created_at
              FROM withdrawals
              WHERE pubkey = $1
              AND EXTRACT(EPOCH FROM created_at)::bigint < $2
              ORDER BY created_at DESC
              LIMIT $3
              "#,
                    pubkey_str,
                    cursor_timestamp,
                    actual_limit + 1
                )
                .fetch_all(self.pool)
                .await?
                .into_iter()
                .map(|record| {
                    // Convert the record to WithdrawalInfo
                    let contract_withdrawal: ContractWithdrawal =
                        serde_json::from_value(record.contract_withdrawal).map_err(|e| {
                            WithdrawalServerError::SerializationError(e.to_string())
                        })?;
                    Ok(WithdrawalInfo {
                        status: record.status.into(),
                        contract_withdrawal,
                        l1_tx_hash: record.l1_tx_hash.map(|h| Bytes32::from_hex(&h).unwrap()),
                        requested_at: record.created_at.timestamp() as u64,
                    })
                })
                .collect::<Result<Vec<_>, WithdrawalServerError>>()?
            }
        };

        let has_more = withdrawal_infos.len() > actual_limit as usize;
        let withdrawal_infos = withdrawal_infos
            .into_iter()
            .take(actual_limit as usize)
            .collect::<Vec<_>>();

        let next_cursor = withdrawal_infos
            .last()
            .map(|withdrawal_info| withdrawal_info.requested_at);

        let total_count = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM withdrawals WHERE pubkey = $1",
            pubkey_str
        )
        .fetch_one(self.pool)
        .await?
        .unwrap_or(0) as u32;

        let cursor_response = TimestampCursorResponse {
            next_cursor,
            has_more,
            total_count,
        };

        Ok((withdrawal_infos, cursor_response))
    }

    pub async fn get_claim_info(
        &self,
        pubkey: U256,
        cursor: TimestampCursor,
    ) -> Result<(Vec<ClaimInfo>, TimestampCursorResponse), WithdrawalServerError> {
        let pubkey_str = pubkey.to_hex();
        let actual_limit = cursor.limit.unwrap_or(100) as i64;

        let claim_infos: Vec<ClaimInfo> = match cursor.order {
            CursorOrder::Asc => {
                let cursor_timestamp = cursor.cursor.unwrap_or(0) as i64;
                sqlx::query!(
                    r#"
                SELECT 
                    status as "status: SqlClaimStatus",
                    claim,
                    submit_claim_proof_tx_hash,
                    l1_tx_hash,
                    created_at
                FROM claims
                WHERE pubkey = $1
                AND EXTRACT(EPOCH FROM created_at)::bigint > $2
                ORDER BY created_at ASC
                LIMIT $3
                "#,
                    pubkey_str,
                    cursor_timestamp,
                    actual_limit + 1
                )
                .fetch_all(self.pool)
                .await?
                .into_iter()
                .map(|record| {
                    let claim: Claim = serde_json::from_value(record.claim)
                        .map_err(|e| WithdrawalServerError::SerializationError(e.to_string()))?;
                    Ok(ClaimInfo {
                        status: record.status.into(),
                        claim,
                        submit_claim_proof_tx_hash: record
                            .submit_claim_proof_tx_hash
                            .map(|h| Bytes32::from_hex(&h).unwrap()),
                        l1_tx_hash: record.l1_tx_hash.map(|h| Bytes32::from_hex(&h).unwrap()),
                        requested_at: record.created_at.timestamp() as u64,
                    })
                })
                .collect::<Result<Vec<_>, WithdrawalServerError>>()?
            }
            CursorOrder::Desc => {
                let cursor_timestamp = cursor.cursor.unwrap_or(i64::MAX as u64) as i64;
                sqlx::query!(
                    r#"
                SELECT 
                    status as "status: SqlClaimStatus",
                    claim,
                    submit_claim_proof_tx_hash,
                    l1_tx_hash,
                    created_at
                FROM claims
                WHERE pubkey = $1
                AND EXTRACT(EPOCH FROM created_at)::bigint < $2
                ORDER BY created_at DESC
                LIMIT $3
                "#,
                    pubkey_str,
                    cursor_timestamp,
                    actual_limit + 1
                )
                .fetch_all(self.pool)
                .await?
                .into_iter()
                .map(|record| {
                    let claim: Claim = serde_json::from_value(record.claim)
                        .map_err(|e| WithdrawalServerError::SerializationError(e.to_string()))?;
                    Ok(ClaimInfo {
                        status: record.status.into(),
                        claim,
                        submit_claim_proof_tx_hash: record
                            .submit_claim_proof_tx_hash
                            .map(|h| Bytes32::from_hex(&h).unwrap()),
                        l1_tx_hash: record.l1_tx_hash.map(|h| Bytes32::from_hex(&h).unwrap()),
                        requested_at: record.created_at.timestamp() as u64,
                    })
                })
                .collect::<Result<Vec<_>, WithdrawalServerError>>()?
            }
        };

        let has_more = claim_infos.len() > actual_limit as usize;
        let claim_infos = claim_infos
            .into_iter()
            .take(actual_limit as usize)
            .collect::<Vec<_>>();

        let next_cursor = claim_infos.last().map(|claim_info| claim_info.requested_at);

        let total_count =
            sqlx::query_scalar!("SELECT COUNT(*) FROM claims WHERE pubkey = $1", pubkey_str)
                .fetch_one(self.pool)
                .await?
                .unwrap_or(0) as u32;

        let cursor_response = TimestampCursorResponse {
            next_cursor,
            has_more,
            total_count,
        };

        Ok((claim_infos, cursor_response))
    }

    pub async fn get_withdrawal_info_by_recipient(
        &self,
        recipient: Address,
        cursor: TimestampCursor,
    ) -> Result<(Vec<WithdrawalInfo>, TimestampCursorResponse), WithdrawalServerError> {
        let recipient_str = recipient.to_hex();
        let actual_limit = cursor.limit.unwrap_or(100) as i64;

        let withdrawal_infos: Vec<WithdrawalInfo> = match cursor.order {
            CursorOrder::Asc => {
                let cursor_timestamp = cursor.cursor.unwrap_or(0) as i64;
                sqlx::query!(
                    r#"
                SELECT 
                    status as "status: SqlWithdrawalStatus",
                    contract_withdrawal,
                    l1_tx_hash,
                    created_at
                FROM withdrawals
                WHERE recipient = $1
                AND EXTRACT(EPOCH FROM created_at)::bigint > $2
                ORDER BY created_at ASC
                LIMIT $3
                "#,
                    recipient_str,
                    cursor_timestamp,
                    actual_limit + 1
                )
                .fetch_all(self.pool)
                .await?
                .into_iter()
                .map(|record| {
                    let contract_withdrawal: ContractWithdrawal =
                        serde_json::from_value(record.contract_withdrawal).map_err(|e| {
                            WithdrawalServerError::SerializationError(e.to_string())
                        })?;
                    Ok(WithdrawalInfo {
                        status: record.status.into(),
                        contract_withdrawal,
                        l1_tx_hash: record.l1_tx_hash.map(|h| Bytes32::from_hex(&h).unwrap()),
                        requested_at: record.created_at.timestamp() as u64,
                    })
                })
                .collect::<Result<Vec<_>, WithdrawalServerError>>()?
            }
            CursorOrder::Desc => {
                let cursor_timestamp = cursor.cursor.unwrap_or(i64::MAX as u64) as i64;
                sqlx::query!(
                    r#"
                SELECT 
                    status as "status: SqlWithdrawalStatus",
                    contract_withdrawal,
                    l1_tx_hash,
                    created_at
                FROM withdrawals
                WHERE recipient = $1
                AND EXTRACT(EPOCH FROM created_at)::bigint < $2
                ORDER BY created_at DESC
                LIMIT $3
                "#,
                    recipient_str,
                    cursor_timestamp,
                    actual_limit + 1
                )
                .fetch_all(self.pool)
                .await?
                .into_iter()
                .map(|record| {
                    let contract_withdrawal: ContractWithdrawal =
                        serde_json::from_value(record.contract_withdrawal).map_err(|e| {
                            WithdrawalServerError::SerializationError(e.to_string())
                        })?;
                    Ok(WithdrawalInfo {
                        status: record.status.into(),
                        contract_withdrawal,
                        l1_tx_hash: record.l1_tx_hash.map(|h| Bytes32::from_hex(&h).unwrap()),
                        requested_at: record.created_at.timestamp() as u64,
                    })
                })
                .collect::<Result<Vec<_>, WithdrawalServerError>>()?
            }
        };

        let has_more = withdrawal_infos.len() > actual_limit as usize;
        let withdrawal_infos = withdrawal_infos
            .into_iter()
            .take(actual_limit as usize)
            .collect::<Vec<_>>();

        let next_cursor = withdrawal_infos
            .last()
            .map(|withdrawal_info| withdrawal_info.requested_at);

        let total_count = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM withdrawals WHERE recipient = $1",
            recipient_str
        )
        .fetch_one(self.pool)
        .await?
        .unwrap_or(0) as u32;

        let cursor_response = TimestampCursorResponse {
            next_cursor,
            has_more,
            total_count,
        };

        Ok((withdrawal_infos, cursor_response))
    }

    pub async fn check_no_duplicated_nullifiers(
        &self,
        transfers: &[Transfer],
    ) -> Result<bool, WithdrawalServerError> {
        let nullifiers: Vec<String> = transfers
            .iter()
            .map(|t| t.nullifier().to_hex())
            .collect::<Vec<_>>();
        let result = sqlx::query!(
            r#"
            SELECT COUNT(*) as count
            FROM used_payments
            WHERE nullifier = ANY($1)
            "#,
            &nullifiers
        )
        .fetch_one(self.pool)
        .await?;
        Ok(result.count.unwrap_or(0) == 0)
    }

    pub async fn add_spent_transfers(
        &self,
        transfers: &[Transfer],
    ) -> Result<(), WithdrawalServerError> {
        log::info!("fee collected: {transfers:?}");
        let nullifiers: Vec<String> = transfers
            .iter()
            .map(|t| t.nullifier().to_hex())
            .collect::<Vec<_>>();
        let transfers: Vec<serde_json::Value> = transfers
            .iter()
            .map(|t| serde_json::to_value(t).unwrap())
            .collect::<Vec<_>>();

        // Batch insert the spent transfers
        match sqlx::query!(
            r#"
        INSERT INTO used_payments (nullifier, transfer)
        SELECT * FROM unnest($1::text[], $2::jsonb[])
        "#,
            &nullifiers,
            &transfers
        )
        .execute(self.pool)
        .await
        {
            Ok(_) => Ok(()),
            Err(e) => {
                if let Some(db_error) = e.as_database_error() {
                    if db_error.code().as_deref() == Some(PG_UNIQUE_VIOLATION_CODE) {
                        return Err(WithdrawalServerError::DuplicateNullifier);
                    }
                }
                Err(e.into())
            }
        }
    }
}
