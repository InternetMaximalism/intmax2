use super::error::WithdrawalServerError;
use crate::api::status::{SqlClaimStatus, SqlWithdrawalStatus};
use intmax2_interfaces::{
    api::withdrawal_server::interface::{ClaimInfo, ContractWithdrawal, WithdrawalInfo},
    data::proof_compression::{CompressedSingleClaimProof, CompressedSingleWithdrawalProof},
    utils::circuit_verifiers::CircuitVerifiers,
};
use intmax2_zkp::{
    circuits::claim::single_claim_processor::SingleClaimProcessor,
    common::{claim::Claim, withdrawal::Withdrawal},
    ethereum_types::{address::Address, u256::U256, u32limb_trait::U32LimbTrait},
    utils::conversion::ToU64,
};
use plonky2::{
    field::goldilocks_field::GoldilocksField,
    plonk::{
        circuit_data::VerifierCircuitData, config::PoseidonGoldilocksConfig,
        proof::ProofWithPublicInputs,
    },
};
use uuid::Uuid;

use server_common::db::{DbPool, DbPoolConfig};

type F = GoldilocksField;
type C = PoseidonGoldilocksConfig;
const D: usize = 2;

pub struct WithdrawalServer {
    pub pool: DbPool,
    pub single_claim_vd: VerifierCircuitData<F, C, D>,
}

impl WithdrawalServer {
    pub async fn new(
        database_url: &str,
        database_max_connections: u32,
        database_timeout: u64,
    ) -> anyhow::Result<Self> {
        let pool = DbPool::from_config(&DbPoolConfig {
            max_connections: database_max_connections,
            idle_timeout: database_timeout,
            url: database_url.to_string(),
        })
        .await?;

        let validity_vd = CircuitVerifiers::load().get_validity_vd();
        let single_claim_processor = SingleClaimProcessor::new(&validity_vd);
        let single_claim_vd = single_claim_processor.get_verifier_data();
        Ok(Self {
            pool,
            single_claim_vd,
        })
    }

    pub async fn request_withdrawal(
        &self,
        pubkey: U256,
        single_withdrawal_proof: &ProofWithPublicInputs<F, C, D>,
    ) -> Result<(), WithdrawalServerError> {
        // Verify the single withdrawal proof
        let single_withdrawal_vd = CircuitVerifiers::load().get_single_withdrawal_vd();
        single_withdrawal_vd
            .verify(single_withdrawal_proof.clone())
            .map_err(|_| WithdrawalServerError::SingleWithdrawalVerificationError)?;

        let withdrawal =
            Withdrawal::from_u64_slice(&single_withdrawal_proof.public_inputs.to_u64_vec());
        let contract_withdrawal = ContractWithdrawal {
            recipient: withdrawal.recipient,
            token_index: withdrawal.token_index,
            amount: withdrawal.amount,
            nullifier: withdrawal.nullifier,
        };
        let withdrawal_hash = contract_withdrawal.withdrawal_hash();
        let withdrawal_hash_str = withdrawal_hash.to_hex();

        // If there is already a request with the same withdrawal_hash, return early
        let existing_request = sqlx::query!(
            r#"
            SELECT COUNT(*) as count
            FROM withdrawals
            WHERE withdrawal_hash = $1
            "#,
            withdrawal_hash_str
        )
        .fetch_one(&self.pool)
        .await?;
        let count = existing_request.count.unwrap_or(0);
        if count > 0 {
            return Ok(());
        }

        // Serialize the proof and public inputs
        let proof_bytes = CompressedSingleWithdrawalProof::new(single_withdrawal_proof)
            .map_err(|e| WithdrawalServerError::SerializationError(e.to_string()))?
            .0;
        let uuid_str = Uuid::new_v4().to_string();

        let pubkey_str = pubkey.to_hex();
        let recipient_str = withdrawal.recipient.to_hex();
        let withdrawal_value = serde_json::to_value(contract_withdrawal)
            .map_err(|e| WithdrawalServerError::SerializationError(e.to_string()))?;
        sqlx::query!(
            r#"
            INSERT INTO withdrawals (
                uuid,
                pubkey,
                recipient,
                withdrawal_hash,
                single_withdrawal_proof,
                contract_withdrawal,
                status
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7::withdrawal_status)
            "#,
            uuid_str,
            pubkey_str,
            recipient_str,
            withdrawal_hash_str,
            proof_bytes,
            withdrawal_value,
            SqlWithdrawalStatus::Requested as SqlWithdrawalStatus
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn request_claim(
        &self,
        pubkey: U256,
        single_claim_proof: &ProofWithPublicInputs<F, C, D>,
    ) -> Result<(), WithdrawalServerError> {
        // Verify the single claim proof
        self.single_claim_vd
            .verify(single_claim_proof.clone())
            .map_err(|_| WithdrawalServerError::SingleClaimVerificationError)?;

        let claim = Claim::from_u64_slice(&single_claim_proof.public_inputs.to_u64_vec());
        let nullifier = claim.nullifier;
        let nullifier_str = nullifier.to_hex();

        // If there is already a request with the same withdrawal_hash, return early
        let existing_request = sqlx::query!(
            r#"
            SELECT COUNT(*) as count
            FROM claims
            WHERE nullifier = $1
            "#,
            nullifier_str
        )
        .fetch_one(&self.pool)
        .await?;
        let count = existing_request.count.unwrap_or(0);
        if count > 0 {
            return Ok(());
        }

        // Serialize the proof and public inputs
        let proof_bytes = CompressedSingleClaimProof::new(single_claim_proof)
            .map_err(|e| WithdrawalServerError::SerializationError(e.to_string()))?
            .0;
        let uuid_str = Uuid::new_v4().to_string();

        let pubkey_str = pubkey.to_hex();
        let recipient_str = claim.recipient.to_hex();
        let nullifier_str = claim.nullifier.to_hex();
        let claim_value = serde_json::to_value(claim)
            .map_err(|e| WithdrawalServerError::SerializationError(e.to_string()))?;
        sqlx::query!(
            r#"
            INSERT INTO claims (
                uuid,
                pubkey,
                recipient,
                nullifier,
                single_claim_proof,
                claim,
                status
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7::claim_status)
            "#,
            uuid_str,
            pubkey_str,
            recipient_str,
            nullifier_str,
            proof_bytes,
            claim_value,
            SqlClaimStatus::Requested as SqlClaimStatus
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_withdrawal_info(
        &self,
        pubkey: U256,
    ) -> Result<Vec<WithdrawalInfo>, WithdrawalServerError> {
        let pubkey_str = pubkey.to_hex();
        let records = sqlx::query!(
            r#"
            SELECT 
                status as "status: SqlWithdrawalStatus",
                contract_withdrawal
            FROM withdrawals
            WHERE pubkey = $1
            "#,
            pubkey_str
        )
        .fetch_all(&self.pool)
        .await?;

        let mut withdrawal_infos = Vec::new();
        for record in records {
            let contract_withdrawal: ContractWithdrawal =
                serde_json::from_value(record.contract_withdrawal)
                    .map_err(|e| WithdrawalServerError::SerializationError(e.to_string()))?;
            withdrawal_infos.push(WithdrawalInfo {
                status: record.status.into(),
                contract_withdrawal,
            });
        }
        Ok(withdrawal_infos)
    }

    pub async fn get_claim_info(
        &self,
        pubkey: U256,
    ) -> Result<Vec<ClaimInfo>, WithdrawalServerError> {
        let pubkey_str = pubkey.to_hex();
        let records = sqlx::query!(
            r#"
            SELECT 
                status as "status: SqlClaimStatus",
                claim
            FROM claims
            WHERE pubkey = $1
            "#,
            pubkey_str
        )
        .fetch_all(&self.pool)
        .await?;

        let mut claim_infos = Vec::new();
        for record in records {
            let claim: Claim = serde_json::from_value(record.claim)
                .map_err(|e| WithdrawalServerError::SerializationError(e.to_string()))?;
            claim_infos.push(ClaimInfo {
                status: record.status.into(),
                claim,
            });
        }
        Ok(claim_infos)
    }

    pub async fn get_withdrawal_info_by_recipient(
        &self,
        recipient: Address,
    ) -> Result<Vec<WithdrawalInfo>, WithdrawalServerError> {
        let recipient_str = recipient.to_hex();
        let records = sqlx::query!(
            r#"
            SELECT 
                status as "status: SqlWithdrawalStatus",
                contract_withdrawal
            FROM withdrawals
            WHERE recipient = $1
            "#,
            recipient_str
        )
        .fetch_all(&self.pool)
        .await?;

        let mut withdrawal_infos = Vec::new();
        for record in records {
            let contract_withdrawal: ContractWithdrawal =
                serde_json::from_value(record.contract_withdrawal)
                    .map_err(|e| WithdrawalServerError::SerializationError(e.to_string()))?;
            withdrawal_infos.push(WithdrawalInfo {
                status: record.status.into(),
                contract_withdrawal,
            });
        }
        Ok(withdrawal_infos)
    }
}
