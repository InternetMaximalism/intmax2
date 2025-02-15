use crate::{
    app::status::{SqlClaimStatus, SqlWithdrawalStatus},
    Env,
};
use intmax2_interfaces::{
    api::store_vault_server::interface::{DataType, StoreVaultClientInterface},
    data::{
        encryption::{errors::EncryptionError, Encryption},
        transfer_data::TransferData,
    },
};

use super::{error::WithdrawalServerError, fee::parse_fee_str};
use ethers::types::H256;
use intmax2_client_sdk::{
    client::{
        fee_payment::FeeType, receive_validation::validate_receive,
        sync::utils::quote_withdrawal_claim_fee,
    },
    external_api::{
        contract::withdrawal_contract::WithdrawalContract,
        store_vault_server::StoreVaultServerClient, validity_prover::ValidityProverClient,
    },
};
use intmax2_interfaces::{
    api::{
        block_builder::interface::Fee,
        withdrawal_server::interface::{
            ClaimFeeInfo, ClaimInfo, ContractWithdrawal, WithdrawalFeeInfo, WithdrawalInfo,
        },
    },
    data::proof_compression::{CompressedSingleClaimProof, CompressedSingleWithdrawalProof},
    utils::circuit_verifiers::CircuitVerifiers,
};
use intmax2_zkp::{
    common::{
        claim::Claim, signature::key_set::KeySet, transfer::Transfer, withdrawal::Withdrawal,
    },
    ethereum_types::{address::Address, bytes32::Bytes32, u256::U256, u32limb_trait::U32LimbTrait},
    utils::conversion::ToU64,
};
use num_bigint::BigUint;
use plonky2::{
    field::goldilocks_field::GoldilocksField,
    plonk::{config::PoseidonGoldilocksConfig, proof::ProofWithPublicInputs},
};
use uuid::Uuid;

use server_common::db::{DbPool, DbPoolConfig};

type F = GoldilocksField;
type C = PoseidonGoldilocksConfig;
const D: usize = 2;

struct Config {
    withdrawal_beneficiary_key: Option<KeySet>,
    claim_beneficiary_key: Option<KeySet>,
    direct_withdrawal_fee: Option<Vec<Fee>>,
    claimable_withdrawal_fee: Option<Vec<Fee>>,
    claim_fee: Option<Vec<Fee>>,
}

pub struct WithdrawalServer {
    config: Config,
    pub pool: DbPool,
    pub store_vault_server: StoreVaultServerClient,
    pub validity_prover: ValidityProverClient,
    pub withdrawal_contract: WithdrawalContract,
}

impl WithdrawalServer {
    pub async fn new(env: &Env) -> anyhow::Result<Self> {
        let pool = DbPool::from_config(&DbPoolConfig {
            max_connections: env.database_max_connections,
            idle_timeout: env.database_timeout,
            url: env.database_url.to_string(),
        })
        .await?;
        let withdrawal_beneficiary_key: Option<KeySet> = env
            .withdrawal_beneficiary_private_key
            .as_ref()
            .map(|&s| privkey_to_keyset(s));
        let direct_withdrawal_fee: Option<Vec<Fee>> = env
            .direct_withdrawal_fee
            .as_ref()
            .map(|fee| parse_fee_str(fee))
            .transpose()?;
        let claimable_withdrawal_fee: Option<Vec<Fee>> = env
            .claimable_withdrawal_fee
            .as_ref()
            .map(|fee| parse_fee_str(fee))
            .transpose()?;
        if (direct_withdrawal_fee.is_some() || claimable_withdrawal_fee.is_some())
            && withdrawal_beneficiary_key.is_none()
        {
            return Err(anyhow::anyhow!("withdrawal fee beneficiary is needed"));
        }
        let claim_beneficiary_key: Option<KeySet> = env
            .claim_beneficiary_private_key
            .as_ref()
            .map(|&s| privkey_to_keyset(s));
        let claim_fee: Option<Vec<Fee>> = env
            .claim_fee
            .as_ref()
            .map(|fee| parse_fee_str(fee))
            .transpose()?;
        if claim_fee.is_some() && claim_beneficiary_key.is_none() {
            return Err(anyhow::anyhow!("claim fee beneficiary is needed"));
        }
        let config = Config {
            withdrawal_beneficiary_key,
            claim_beneficiary_key,
            direct_withdrawal_fee,
            claimable_withdrawal_fee,
            claim_fee,
        };
        let store_vault_server = StoreVaultServerClient::new(&env.store_vault_server_base_url);
        let validity_prover = ValidityProverClient::new(&env.validity_prover_base_url);
        let withdrawal_contract = WithdrawalContract::new(
            &env.l2_rpc_url,
            env.l2_chain_id,
            env.withdrawal_contract_address,
        );

        Ok(Self {
            config,
            pool,
            store_vault_server,
            validity_prover,
            withdrawal_contract,
        })
    }

    pub fn get_withdrawal_fee(&self) -> WithdrawalFeeInfo {
        WithdrawalFeeInfo {
            beneficiary: self.config.withdrawal_beneficiary_key.map(|k| k.pubkey),
            direct_withdrawal_fee: self.config.direct_withdrawal_fee.clone(),
            claimable_withdrawal_fee: self.config.claimable_withdrawal_fee.clone(),
        }
    }

    pub fn get_claim_fee(&self) -> ClaimFeeInfo {
        ClaimFeeInfo {
            beneficiary: self.config.claim_beneficiary_key.map(|k| k.pubkey),
            fee: self.config.claim_fee.clone(),
        }
    }

    pub async fn request_withdrawal(
        &self,
        pubkey: U256,
        single_withdrawal_proof: &ProofWithPublicInputs<F, C, D>,
        fee_token_index: Option<u32>,
        fee_transfer_uuids: &[String],
    ) -> Result<(), WithdrawalServerError> {
        // Verify the single withdrawal proof
        let single_withdrawal_vd = CircuitVerifiers::load().get_single_withdrawal_vd();
        single_withdrawal_vd
            .verify(single_withdrawal_proof.clone())
            .map_err(|_| WithdrawalServerError::SingleWithdrawalVerificationError)?;

        let withdrawal =
            Withdrawal::from_u64_slice(&single_withdrawal_proof.public_inputs.to_u64_vec());

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
            let transfers = self
                .fee_validation(FeeType::Withdrawal, &fee, fee_transfer_uuids)
                .await?;
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
        fee_token_index: Option<u32>,
        fee_transfer_uuids: &[String],
    ) -> Result<(), WithdrawalServerError> {
        let claim = Claim::from_u64_slice(&single_claim_proof.public_inputs.to_u64_vec());
        let nullifier = claim.nullifier;
        let nullifier_str = nullifier.to_hex();

        // validate fee
        let fee = quote_withdrawal_claim_fee(fee_token_index, self.config.claim_fee.clone())
            .map_err(|e| WithdrawalServerError::InvalidFee(e.to_string()))?;
        if let Some(fee) = fee {
            let transfers = self
                .fee_validation(FeeType::Claim, &fee, fee_transfer_uuids)
                .await?;
            self.add_spent_transfers(&transfers).await?;
        }

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

    async fn fee_validation(
        &self,
        fee_type: FeeType,
        fee: &Fee,
        fee_transfer_uuids: &[String],
    ) -> Result<Vec<Transfer>, WithdrawalServerError> {
        let key = match fee_type {
            FeeType::Withdrawal => self.config.withdrawal_beneficiary_key.unwrap(),
            FeeType::Claim => self.config.claim_beneficiary_key.unwrap(),
        };
        // fetch transfer data
        let encrypted_transfer_data = self
            .store_vault_server
            .get_data_batch(key, DataType::Transfer, fee_transfer_uuids)
            .await?;
        if encrypted_transfer_data.len() != fee_transfer_uuids.len() {
            return Err(WithdrawalServerError::InvalidFee(format!(
                "Invalid fee transfer uuid response: expected {}, got {}",
                fee_transfer_uuids.len(),
                encrypted_transfer_data.len()
            )));
        }

        let transfer_data_with_meta = encrypted_transfer_data
            .iter()
            .map(|data| {
                let transfer_data = TransferData::decrypt(&data.data, key)?;
                Ok((data.meta.clone(), transfer_data))
            })
            .collect::<Result<Vec<_>, EncryptionError>>()?;

        let mut collected_fee = U256::zero();
        let mut transfers = Vec::new();
        for (meta, transfer_data) in transfer_data_with_meta {
            let transfer = validate_receive(
                &self.store_vault_server,
                &self.validity_prover,
                key.pubkey,
                &meta,
                &transfer_data,
            )
            .await?;
            if fee.token_index != transfer.token_index {
                return Err(WithdrawalServerError::InvalidFee(format!(
                    "Invalid fee token index: expected {}, got {}",
                    fee.token_index, transfer.token_index
                )));
            }
            collected_fee += transfer.amount;
            transfers.push(transfer);
        }
        if collected_fee < fee.amount {
            return Err(WithdrawalServerError::InvalidFee(format!(
                "Insufficient fee: expected {}, got {}",
                fee.amount, collected_fee
            )));
        }
        Ok(transfers)
    }

    async fn add_spent_transfers(
        &self,
        transfers: &[Transfer],
    ) -> Result<(), WithdrawalServerError> {
        log::info!("fee collected: {:?}", transfers);
        let nullifiers: Vec<String> = transfers
            .iter()
            .map(|t| Bytes32::from(t.commitment()).to_hex())
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
        .execute(&self.pool)
        .await
        {
            Ok(_) => Ok(()),
            Err(e) => {
                if let Some(db_error) = e.as_database_error() {
                    if db_error.code().as_deref() == Some("23505") {
                        return Err(WithdrawalServerError::DuplicateNullifier);
                    }
                }
                Err(e.into())
            }
        }
    }
}

pub fn privkey_to_keyset(privkey: H256) -> KeySet {
    KeySet::new(BigUint::from_bytes_be(privkey.as_bytes()).into())
}
