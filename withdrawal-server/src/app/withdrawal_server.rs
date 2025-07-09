use std::sync::Arc;

use crate::{
    app::{
        config::Config,
        db_operations::DbOperations,
        fee_handler::FeeHandler,
        status::{SqlClaimStatus, SqlWithdrawalStatus},
        validator::{BlockHashValidator, RealBlockHashValidator},
    },
    Env,
};
use alloy::primitives::B256;
use intmax2_interfaces::{
    api::{
        store_vault_server::interface::StoreVaultClientInterface,
        withdrawal_server::{
            interface::FeeResult,
            types::{TimestampCursor, TimestampCursorResponse},
        },
    },
    utils::{address::IntmaxAddress, fee::Fee},
};

use super::error::WithdrawalServerError;
use intmax2_client_sdk::{
    client::{fee_payment::FeeType, sync::utils::quote_withdrawal_claim_fee},
    external_api::{
        contract::{
            convert::convert_b256_to_bytes32, rollup_contract::RollupContract,
            utils::NormalProvider, withdrawal_contract::WithdrawalContract,
        },
        s3_store_vault::S3StoreVaultClient,
        store_vault_server::StoreVaultServerClient,
        validity_prover::ValidityProverClient,
    },
};
use intmax2_interfaces::{
    api::withdrawal_server::interface::{
        ClaimFeeInfo, ClaimInfo, ContractWithdrawal, WithdrawalFeeInfo, WithdrawalInfo,
    },
    data::proof_compression::{CompressedSingleClaimProof, CompressedSingleWithdrawalProof},
    utils::circuit_verifiers::CircuitVerifiers,
};
use intmax2_zkp::{
    common::{
        claim::Claim, signature_content::key_set::KeySet, transfer::Transfer,
        withdrawal::Withdrawal,
    },
    ethereum_types::{address::Address, bytes32::Bytes32, u256::U256, u32limb_trait::U32LimbTrait},
    utils::conversion::ToU64,
};
use plonky2::{
    field::goldilocks_field::GoldilocksField,
    plonk::{config::PoseidonGoldilocksConfig, proof::ProofWithPublicInputs},
};

use server_common::db::{DbPool, DbPoolConfig};

type F = GoldilocksField;
type C = PoseidonGoldilocksConfig;
const D: usize = 2;

pub struct WithdrawalServer {
    config: Config,
    pub pool: DbPool,
    pub store_vault_server: Box<dyn StoreVaultClientInterface>,
    pub validity_prover: ValidityProverClient,
    pub rollup_contract: RollupContract,
    pub withdrawal_contract: WithdrawalContract,
    pub validator: Arc<dyn BlockHashValidator>,
}

impl WithdrawalServer {
    /// Creates a new instance of WithdrawalServer
    ///
    /// Uses Postgres image and requires 'event' and 'withdrawal' databases in it.
    ///
    /// # Arguments
    /// * `env` - Environment variable with the necessary settings
    ///
    /// # Returns
    /// * `Result(Self)` - The instance itself or the error
    pub async fn new(env: &Env, provider: NormalProvider) -> anyhow::Result<Self> {
        let validator = Arc::new(RealBlockHashValidator);
        Self::new_with_validator(env, provider, validator).await
    }

    pub async fn new_with_validator(
        env: &Env,
        provider: NormalProvider,
        validator: Arc<dyn BlockHashValidator>,
    ) -> anyhow::Result<Self> {
        let pool = DbPool::from_config(&DbPoolConfig {
            max_connections: env.database_max_connections,
            idle_timeout: env.database_timeout,
            url: env.database_url.to_string(),
        })
        .await?;

        let config = Config::from_env(env)?;

        let store_vault_server: Box<dyn StoreVaultClientInterface> = if env.use_s3.unwrap_or(true) {
            log::info!("Using s3_store_vault");
            Box::new(S3StoreVaultClient::new(&env.store_vault_server_base_url))
        } else {
            log::info!("Using store_vault_server");
            Box::new(StoreVaultServerClient::new(
                &env.store_vault_server_base_url,
            ))
        };
        let validity_prover = ValidityProverClient::new(&env.validity_prover_base_url);
        let rollup_contract = RollupContract::new(provider.clone(), env.rollup_contract_address);
        let withdrawal_contract =
            WithdrawalContract::new(provider, env.withdrawal_contract_address);

        Ok(Self {
            config,
            pool,
            store_vault_server,
            validity_prover,
            rollup_contract,
            withdrawal_contract,
            validator,
        })
    }

    pub fn get_withdrawal_fee(&self) -> WithdrawalFeeInfo {
        WithdrawalFeeInfo {
            beneficiary: IntmaxAddress::from_viewpair(
                self.config.network,
                &self.config.withdrawal_beneficiary_key,
            ),
            direct_withdrawal_fee: self.config.direct_withdrawal_fee.clone(),
            claimable_withdrawal_fee: self.config.claimable_withdrawal_fee.clone(),
        }
    }

    pub fn get_claim_fee(&self) -> ClaimFeeInfo {
        ClaimFeeInfo {
            beneficiary: IntmaxAddress::from_viewpair(
                self.config.network,
                &self.config.claim_beneficiary_key,
            ),
            fee: self.config.claim_fee.clone(),
        }
    }

    pub async fn request_withdrawal(
        &self,
        pubkey: U256,
        single_withdrawal_proof: &ProofWithPublicInputs<F, C, D>,
        fee_token_index: Option<u32>,
        fee_transfer_digests: &[Bytes32],
    ) -> Result<FeeResult, WithdrawalServerError> {
        // Verify the single withdrawal proof
        let single_withdrawal_vd = CircuitVerifiers::load().get_single_withdrawal_vd();
        single_withdrawal_vd
            .verify(single_withdrawal_proof.clone())
            .map_err(|_| WithdrawalServerError::SingleWithdrawalVerificationError)?;

        let withdrawal =
            Withdrawal::from_u64_slice(&single_withdrawal_proof.public_inputs.to_u64_vec())
                .map_err(|e| WithdrawalServerError::SerializationError(e.to_string()))?;

        // validate block hash existence
        self.validator
            .validate_block_hash_existence(
                &self.rollup_contract,
                withdrawal.block_number,
                withdrawal.block_hash,
            )
            .await?;

        // validate fee
        let direct_withdrawal_tokens = self
            .withdrawal_contract
            .get_direct_withdrawal_token_indices()
            .await?;
        let fees = if direct_withdrawal_tokens.contains(&withdrawal.token_index) {
            self.config.direct_withdrawal_fee.clone()
        } else {
            self.config.claimable_withdrawal_fee.clone()
        };
        let fee = quote_withdrawal_claim_fee(fee_token_index, fees)
            .map_err(|e| WithdrawalServerError::InvalidFee(e.to_string()))?;

        if let Some(fee) = fee {
            let (transfers, fee_result) = self
                .fee_validation(FeeType::Withdrawal, &fee, fee_transfer_digests)
                .await?;
            if fee_result != FeeResult::Success {
                return Ok(fee_result);
            }
            self.add_spent_transfers(&transfers).await?;
        }

        let contract_withdrawal = ContractWithdrawal {
            recipient: withdrawal.recipient,
            token_index: withdrawal.token_index,
            amount: withdrawal.amount,
            nullifier: withdrawal.nullifier,
        };
        let withdrawal_hash = contract_withdrawal.withdrawal_hash();
        let withdrawal_hash_str = withdrawal_hash.to_hex();

        // If there is already a request with the same withdrawal_hash, return early
        let already_exists: (bool,) = sqlx::query_as::<_, (bool,)>(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM withdrawals
                WHERE withdrawal_hash = $1
            )
            "#,
        )
        .bind(&withdrawal_hash_str)
        .fetch_one(&self.pool)
        .await?;
        if already_exists.0 {
            return Ok(FeeResult::Success);
        }

        // Serialize the proof and public inputs
        let proof_bytes = CompressedSingleWithdrawalProof::new(single_withdrawal_proof)
            .map_err(|e| WithdrawalServerError::SerializationError(e.to_string()))?
            .0;

        let pubkey_str = pubkey.to_hex();
        let recipient_str = withdrawal.recipient.to_hex();
        let withdrawal_value = serde_json::to_value(contract_withdrawal)
            .map_err(|e| WithdrawalServerError::SerializationError(e.to_string()))?;
        sqlx::query!(
            r#"
            INSERT INTO withdrawals (
                pubkey,
                recipient,
                withdrawal_hash,
                single_withdrawal_proof,
                contract_withdrawal,
                status
            )
            VALUES ($1, $2, $3, $4, $5, $6::withdrawal_status)
            "#,
            pubkey_str,
            recipient_str,
            withdrawal_hash_str,
            proof_bytes,
            withdrawal_value,
            SqlWithdrawalStatus::Requested as SqlWithdrawalStatus
        )
        .execute(&self.pool)
        .await?;

        Ok(FeeResult::Success)
    }

    pub async fn request_claim(
        &self,
        pubkey: U256,
        single_claim_proof: &ProofWithPublicInputs<F, C, D>,
        fee_token_index: Option<u32>,
        fee_transfer_digests: &[Bytes32],
    ) -> Result<FeeResult, WithdrawalServerError> {
        let claim_verifier = CircuitVerifiers::load().get_claim_vd(self.config.is_faster_mining);
        claim_verifier
            .verify(single_claim_proof.clone())
            .map_err(|_| WithdrawalServerError::SingleClaimVerificationError)?;
        let claim = Claim::from_u64_slice(&single_claim_proof.public_inputs.to_u64_vec())
            .map_err(|e| WithdrawalServerError::SerializationError(e.to_string()))?;

        // validate block hash existence
        self.validator
            .validate_block_hash_existence(
                &self.rollup_contract,
                claim.block_number,
                claim.block_hash,
            )
            .await?;

        let nullifier = claim.nullifier;
        let nullifier_str = nullifier.to_hex();

        // validate fee
        let fee = quote_withdrawal_claim_fee(fee_token_index, self.config.claim_fee.clone())
            .map_err(|e| WithdrawalServerError::InvalidFee(e.to_string()))?;
        if let Some(fee) = fee {
            let (transfers, fee_result) = self
                .fee_validation(FeeType::Claim, &fee, fee_transfer_digests)
                .await?;
            if fee_result != FeeResult::Success {
                return Ok(fee_result);
            }
            self.add_spent_transfers(&transfers).await?;
        }

        // If there is already a request with the same nullifier_str, return early
        let already_exists: (bool,) = sqlx::query_as::<_, (bool,)>(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM claims
                WHERE nullifier = $1
            )
            "#,
        )
        .bind(&nullifier_str)
        .fetch_one(&self.pool)
        .await?;
        if already_exists.0 {
            return Ok(FeeResult::Success);
        }

        // Serialize the proof and public inputs
        let proof_bytes = CompressedSingleClaimProof::new(single_claim_proof)
            .map_err(|e| WithdrawalServerError::SerializationError(e.to_string()))?
            .0;
        let pubkey_str = pubkey.to_hex();
        let recipient_str = claim.recipient.to_hex();
        let nullifier_str = claim.nullifier.to_hex();
        let claim_value = serde_json::to_value(claim)
            .map_err(|e| WithdrawalServerError::SerializationError(e.to_string()))?;
        sqlx::query!(
            r#"
            INSERT INTO claims (
                pubkey,
                recipient,
                nullifier,
                single_claim_proof,
                claim,
                status
            )
            VALUES ($1, $2, $3, $4, $5, $6::claim_status)
            "#,
            pubkey_str,
            recipient_str,
            nullifier_str,
            proof_bytes,
            claim_value,
            SqlClaimStatus::Requested as SqlClaimStatus
        )
        .execute(&self.pool)
        .await?;

        Ok(FeeResult::Success)
    }

    pub async fn get_withdrawal_info(
        &self,
        pubkey: U256,
        cursor: TimestampCursor,
    ) -> Result<(Vec<WithdrawalInfo>, TimestampCursorResponse), WithdrawalServerError> {
        let db_ops = DbOperations::new(&self.pool);
        db_ops.get_withdrawal_info(pubkey, cursor).await
    }

    pub async fn get_claim_info(
        &self,
        pubkey: U256,
        cursor: TimestampCursor,
    ) -> Result<(Vec<ClaimInfo>, TimestampCursorResponse), WithdrawalServerError> {
        let db_ops = DbOperations::new(&self.pool);
        db_ops.get_claim_info(pubkey, cursor).await
    }

    pub async fn get_withdrawal_info_by_recipient(
        &self,
        recipient: Address,
        cursor: TimestampCursor,
    ) -> Result<(Vec<WithdrawalInfo>, TimestampCursorResponse), WithdrawalServerError> {
        let db_ops = DbOperations::new(&self.pool);
        db_ops
            .get_withdrawal_info_by_recipient(recipient, cursor)
            .await
    }

    async fn fee_validation(
        &self,
        fee_type: FeeType,
        fee: &Fee,
        fee_transfer_digests: &[Bytes32],
    ) -> Result<(Vec<Transfer>, FeeResult), WithdrawalServerError> {
        let db_ops = DbOperations::new(&self.pool);
        let fee_handler = FeeHandler::new(
            &self.config,
            db_ops,
            self.store_vault_server.as_ref(),
            &self.validity_prover,
        );
        fee_handler
            .validate_fee(fee_type, fee, fee_transfer_digests)
            .await
    }

    async fn add_spent_transfers(
        &self,
        transfers: &[Transfer],
    ) -> Result<(), WithdrawalServerError> {
        let db_ops = DbOperations::new(&self.pool);
        db_ops.add_spent_transfers(transfers).await
    }
}

pub fn privkey_to_keyset(privkey: B256) -> KeySet {
    let privkey: Bytes32 = convert_b256_to_bytes32(privkey);
    KeySet::new(privkey.into())
}
