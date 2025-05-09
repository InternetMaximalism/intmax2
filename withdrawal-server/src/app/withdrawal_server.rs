use crate::{
    app::status::{SqlClaimStatus, SqlWithdrawalStatus},
    Env,
};
use alloy::primitives::B256;
use intmax2_interfaces::{
    api::{
        store_vault_server::interface::StoreVaultClientInterface,
        withdrawal_server::interface::FeeResult,
    },
    data::{
        data_type::DataType,
        encryption::{errors::BlsEncryptionError, BlsEncryption},
        transfer_data::TransferData,
    },
};

use super::{error::WithdrawalServerError, fee::parse_optional_fee_str};
use intmax2_client_sdk::{
    client::{
        fee_payment::FeeType,
        receive_validation::{validate_receive, ReceiveValidationError},
        sync::utils::quote_withdrawal_claim_fee,
    },
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

struct Config {
    is_faster_mining: bool,
    withdrawal_beneficiary_key: Option<KeySet>,
    claim_beneficiary_key: Option<KeySet>,
    direct_withdrawal_fee: Option<Vec<Fee>>,
    claimable_withdrawal_fee: Option<Vec<Fee>>,
    claim_fee: Option<Vec<Fee>>,
}

impl Config {
    pub fn from_env(env: &Env) -> Result<Self, WithdrawalServerError> {
        let withdrawal_beneficiary_key = env
            .withdrawal_beneficiary_private_key
            .as_ref()
            .map(|&key| privkey_to_keyset(key));

        let direct_withdrawal_fee = parse_optional_fee_str(&env.direct_withdrawal_fee)?;
        let claimable_withdrawal_fee: Option<Vec<Fee>> =
            parse_optional_fee_str(&env.claimable_withdrawal_fee)?;
        if (direct_withdrawal_fee.is_some() || claimable_withdrawal_fee.is_some())
            && withdrawal_beneficiary_key.is_none()
        {
            return Err(WithdrawalServerError::ConfigError(
                "Withdrawal fee beneficiary is needed".to_string(),
            ));
        }

        let claim_beneficiary_key: Option<KeySet> = env
            .claim_beneficiary_private_key
            .as_ref()
            .map(|&s| privkey_to_keyset(s));
        let claim_fee: Option<Vec<Fee>> = parse_optional_fee_str(&env.claim_fee)?;
        if claim_fee.is_some() && claim_beneficiary_key.is_none() {
            return Err(WithdrawalServerError::ConfigError(
                "Claim fee beneficiary is needed".to_string(),
            ));
        }

        Ok(Self {
            is_faster_mining: env.is_faster_mining,
            withdrawal_beneficiary_key,
            claim_beneficiary_key,
            direct_withdrawal_fee,
            claimable_withdrawal_fee,
            claim_fee,
        })
    }
}

pub struct WithdrawalServer {
    config: Config,
    pub pool: DbPool,
    pub store_vault_server: Box<dyn StoreVaultClientInterface>,
    pub validity_prover: ValidityProverClient,
    pub rollup_contract: RollupContract,
    pub withdrawal_contract: WithdrawalContract,
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
        Self::validate_block_hash_existence(
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
        let uuid_str = uuid::Uuid::new_v4().to_string();
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
        Self::validate_block_hash_existence(
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
        let uuid_str = uuid::Uuid::new_v4().to_string();
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

        Ok(FeeResult::Success)
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
                contract_withdrawal,
                l1_tx_hash
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
                l1_tx_hash: record.l1_tx_hash.map(|h| Bytes32::from_hex(&h).unwrap()),
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
                claim,
                l1_tx_hash
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
                l1_tx_hash: record.l1_tx_hash.map(|h| Bytes32::from_hex(&h).unwrap()),
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
                contract_withdrawal,
                l1_tx_hash
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
                l1_tx_hash: record.l1_tx_hash.map(|h| Bytes32::from_hex(&h).unwrap()),
            });
        }
        Ok(withdrawal_infos)
    }

    async fn fee_validation(
        &self,
        fee_type: FeeType,
        fee: &Fee,
        fee_transfer_digests: &[Bytes32],
    ) -> Result<(Vec<Transfer>, FeeResult), WithdrawalServerError> {
        // check duplicated nullifiers

        let key = match fee_type {
            FeeType::Withdrawal => self.config.withdrawal_beneficiary_key.unwrap(),
            FeeType::Claim => self.config.claim_beneficiary_key.unwrap(),
        };
        // fetch transfer data
        let encrypted_transfer_data = self
            .store_vault_server
            .get_data_batch(key, &DataType::Transfer.to_topic(), fee_transfer_digests)
            .await?;
        if encrypted_transfer_data.len() != fee_transfer_digests.len() {
            return Err(WithdrawalServerError::InvalidFee(format!(
                "Invalid fee transfer digest response: expected {}, got {}",
                fee_transfer_digests.len(),
                encrypted_transfer_data.len()
            )));
        }

        let transfer_data_with_meta = encrypted_transfer_data
            .iter()
            .map(|data| {
                let transfer_data = TransferData::decrypt(key, None, &data.data)?;
                Ok((data.meta.clone(), transfer_data))
            })
            .collect::<Result<Vec<_>, BlsEncryptionError>>();
        let transfer_data_with_meta = match transfer_data_with_meta {
            Ok(data) => data,
            Err(e) => {
                log::warn!("Failed to decrypt transfer data: {}", e);
                return Ok((Vec::new(), FeeResult::DecryptionError));
            }
        };

        let mut collected_fee = U256::zero();
        let mut transfers = Vec::new();
        for (meta, transfer_data) in transfer_data_with_meta {
            let transfer = match validate_receive(
                self.store_vault_server.as_ref(),
                &self.validity_prover,
                key.pubkey,
                &meta,
                &transfer_data,
            )
            .await
            {
                Ok(transfer) => transfer,
                Err(e) => {
                    if matches!(e, ReceiveValidationError::ValidationError(_)) {
                        return Ok((Vec::new(), FeeResult::ValidationError));
                    } else {
                        return Err(e.into());
                    }
                }
            };
            if fee.token_index != transfer.token_index {
                return Ok((Vec::new(), FeeResult::TokenIndexMismatch));
            }
            collected_fee += transfer.amount;
            transfers.push(transfer);
        }
        if collected_fee < fee.amount {
            return Ok((Vec::new(), FeeResult::Insufficient));
        }
        if !self.check_no_duplicated_nullifiers(&transfers).await? {
            return Ok((Vec::new(), FeeResult::AlreadyUsed));
        }
        Ok((transfers, FeeResult::Success))
    }

    async fn check_no_duplicated_nullifiers(
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
        .fetch_one(&self.pool)
        .await?;
        Ok(result.count.unwrap_or(0) == 0)
    }

    async fn add_spent_transfers(
        &self,
        transfers: &[Transfer],
    ) -> Result<(), WithdrawalServerError> {
        log::info!("fee collected: {:?}", transfers);
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

    // Helper methods
    async fn validate_block_hash_existence(
        contract: &RollupContract,
        block_number: u32,
        expected_hash: Bytes32,
    ) -> Result<(), WithdrawalServerError> {
        let onchain_hash = contract.get_block_hash(block_number).await?;
        if onchain_hash != expected_hash {
            return Err(WithdrawalServerError::InvalidBlockHash(format!(
                "Invalid block hash: expected {}, got {} at block number {}",
                expected_hash.to_hex(),
                onchain_hash.to_hex(),
                block_number
            )));
        }
        Ok(())
    }
}

pub fn privkey_to_keyset(privkey: B256) -> KeySet {
    let privkey: Bytes32 = convert_b256_to_bytes32(privkey);
    KeySet::new(privkey.into())
}

#[cfg(test)]
pub mod test_withdrawal_server_helper {
    use std::{fs, io::Read, panic};
    // For redis
    use std::{
        net::TcpListener,
        process::{Command, Output, Stdio},
    };

    use server_common::db::DbPool;
    use sqlx::query;

    pub fn run_withdrawal_docker(port: u16, container_name: &str) -> Output {
        let port_arg = format!("{}:5432", port);

        let output = Command::new("docker")
            .args([
                "run",
                "-d",
                "--rm",
                "--name",
                container_name,
                "--hostname",
                "--postgres",
                "-e",
                "POSTGRES_USER=postgres",
                "-e",
                "POSTGRES_PASSWORD=password",
                "-e",
                "POSTGRES_DB=maindb",
                "-p",
                &port_arg,
                "postgres:16.6",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .expect("Error during Redis container startup");

        output
    }

    pub fn create_databases(container_name: &str) {
        let commands = ["CREATE DATABASE event;", "CREATE DATABASE withdrawal;"];

        for sql_cmd in commands {
            let status = Command::new("docker")
                .args([
                    "exec",
                    "-i", // No TTY needed; `-it` is for interactive terminal; `-i` is enough here
                    container_name,
                    "psql",
                    "-U",
                    "postgres",
                    "-d",
                    "maindb",
                    "-c",
                    sql_cmd,
                ])
                .status()
                .expect("Failed to execute docker exec");

            assert!(status.success(), "Couldn't run {}", sql_cmd);
        }
    }

    pub async fn create_tables(pool: &DbPool, file_path: &str) {
        // Open and read file
        let mut file =
            fs::File::open(file_path).unwrap_or_else(|e| panic!("Failed to open SQL file: {}", e));
        let mut sql_content = String::new();
        file.read_to_string(&mut sql_content)
            .unwrap_or_else(|e| panic!("Failed to read SQL file: {}", e));

        // Execute the SQL content
        for statement in sql_content.split(';') {
            let trimmed = statement.trim();
            if !trimmed.is_empty() {
                query(trimmed)
                    .execute(pool)
                    .await
                    .unwrap_or_else(|e| panic!("Failed to execute SQL: {}", e));
            }
        }
    }

    pub fn stop_withdrawal_docker(container_name: &str) -> Output {
        let output = Command::new("docker")
            .args(["stop", container_name])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .expect("Error during Redis container stopping");

        output
    }

    pub fn find_free_port() -> u16 {
        TcpListener::bind("127.0.0.1:0")
            .expect("Failed to bind to address")
            .local_addr()
            .unwrap()
            .port()
    }

    pub fn assert_and_stop<F: FnOnce() + panic::UnwindSafe>(cont_name: &str, f: F) {
        let res = panic::catch_unwind(f);

        if let Err(panic_info) = res {
            stop_withdrawal_docker(cont_name);
            panic::resume_unwind(panic_info);
        }
    }
}

#[cfg(test)]
mod tests {
    use alloy::{
        primitives::Address,
        providers::{mock::Asserter, ProviderBuilder},
    };
    use intmax2_zkp::ethereum_types::u256::U256;
    use serde_json::json;
    use std::{str::FromStr as _, thread::sleep, time::Duration};

    use crate::{
        app::{
            fee::parse_fee_str,
            withdrawal_server::test_withdrawal_server_helper::{
                assert_and_stop, create_databases, create_tables, find_free_port,
                run_withdrawal_docker, stop_withdrawal_docker,
            },
        },
        Env,
    };

    use super::*;

    fn get_provider() -> NormalProvider {
        let provider_asserter = Asserter::new();
        ProviderBuilder::default()
            .with_gas_estimation()
            .with_simple_nonce_management()
            .fetch_chain_id()
            .connect_mocked_client(provider_asserter)
    }

    fn get_example_env() -> Env {
        Env {
            port: 9003,
            database_url: "postgres://postgres:password@localhost:5432/withdrawal".to_string(),
            database_max_connections: 10,
            database_timeout: 10,

            store_vault_server_base_url: "http://localhost:9000".to_string(),
            use_s3: Some(true),
            validity_prover_base_url: "http://localhost:9002".to_string(),

            l2_rpc_url: "http://127.0.0.1:8545".to_string(),
            rollup_contract_address: Address::from_str(
                "0xe7f1725e7734ce288f8367e1bb143e90bb3f0512",
            )
            .unwrap(),
            withdrawal_contract_address: Address::from_str(
                "0x8a791620dd6260079bf849dc5567adc3f2fdc318",
            )
            .unwrap(),

            is_faster_mining: true,
            withdrawal_beneficiary_private_key: Some(
                B256::from_str(
                    "0x1a1ef1bc29051c687773b8751961827400215d295e4ee2ef8754c7f831a3b447",
                )
                .unwrap(),
            ),
            claim_beneficiary_private_key: Some(
                B256::from_str(
                    "0x1a1ef1bc29051c687773b8751961827400215d295e4ee2ef8754c7f831a3b447",
                )
                .unwrap(),
            ),
            direct_withdrawal_fee: Some("0:100".to_string()),
            claimable_withdrawal_fee: Some("0:10".to_string()),
            claim_fee: Some("0:100".to_string()),
        }
    }

    #[tokio::test]
    async fn test_getting_fee() {
        // We use a port different from the default one (5432)
        let port = find_free_port();
        let cont_name = "withdrawal-test-getting-fee";

        stop_withdrawal_docker(cont_name);
        let output = run_withdrawal_docker(port, cont_name);
        assert!(
            output.status.success(),
            "Couldn't start {}: {}",
            cont_name,
            String::from_utf8_lossy(&output.stderr)
        );

        // 2.5 seconds should be enough for postgres container to be started to create databases
        sleep(Duration::from_millis(2500));
        assert_and_stop(cont_name, || create_databases(cont_name));

        let mut env = get_example_env();
        env.database_url =
            format!("postgres://postgres:password@localhost:{}/withdrawal", port).to_string();
        let server = WithdrawalServer::new(&env, get_provider()).await;

        if let Err(err) = &server {
            stop_withdrawal_docker(cont_name);
            panic!("Withdrawal Server initialization failed: {:?}", err);
        }
        let server = server.unwrap();

        // Create needed SQL tables
        create_tables(
            &server.pool,
            "./migrations/20250309092609_create_initial_tables.up.sql",
        )
        .await;

        // Test get_claim_fee and get_withdrawal_fee
        {
            // Here and later I use is_some() || is_some() and not && as an additional check of initializing WithdrawalServer.
            // If only one variable is Some and another one is not, test will fail, so there is should be some error in WithdrawalServer new method.
            let claim_fee = server.get_claim_fee();
            if env.claim_beneficiary_private_key.is_some() || claim_fee.beneficiary.is_some() {
                let claim_keyset = privkey_to_keyset(env.claim_beneficiary_private_key.unwrap());
                assert_and_stop(cont_name, || {
                    assert_eq!(claim_fee.beneficiary.unwrap(), claim_keyset.pubkey)
                });
            }
            if env.claim_fee.is_some() {
                let fee = parse_fee_str(&env.claim_fee.unwrap()).unwrap();
                assert_and_stop(cont_name, || assert_eq!(claim_fee.fee.unwrap(), fee));
            }

            let withdrawal_fee = server.get_withdrawal_fee();
            if withdrawal_fee.beneficiary.is_some()
                || env.withdrawal_beneficiary_private_key.is_some()
            {
                let ben_keyset = privkey_to_keyset(env.withdrawal_beneficiary_private_key.unwrap());
                assert_and_stop(cont_name, || {
                    assert_eq!(withdrawal_fee.beneficiary.unwrap(), ben_keyset.pubkey)
                });
            }
            if withdrawal_fee.direct_withdrawal_fee.is_some() {
                assert_and_stop(cont_name, || {
                    assert_eq!(withdrawal_fee.direct_withdrawal_fee.unwrap().len(), 1)
                });
            }
        }

        // Test inserting and checking withdrawal and claim tables for needed hash
        {
            let pubkey_str = U256::from_hex(
                "0xdeadbeef29051c687773b8751961827400215d295e4ee2ef8754c7f831a3b447",
            )
            .unwrap();
            let recipient_str = "0xabc";
            let withdrawal_hash = "0xdeadbeef";
            let proof_bytes = vec![1u8, 2, 3, 4]; // Replace with actual proof if needed
            let claim_value = json!({
                "recipient": recipient_str,
                "amount": "1000",
                "token_index": 1,
                "block_number": 42,
                "block_hash": "0xblockhash",
                "nullifier": withdrawal_hash
            });
            let uuid_str = uuid::Uuid::new_v4().to_string();

            // Check claims table for some withdrawal_hash record
            let exists: (bool,) = sqlx::query_as::<_, (bool,)>(
                r#"
                SELECT EXISTS(
                    SELECT 1 FROM withdrawals WHERE withdrawal_hash = $1
                )
                "#,
            )
            .bind(withdrawal_hash)
            .fetch_one(&server.pool)
            .await
            .expect("Failed to check existence of withdrawal_hash in claims table");

            assert_and_stop(cont_name, || {
                assert!(!exists.0, "Claim should not contain withdrawal_hash")
            });

            sqlx::query(
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
            )
            .bind(&uuid_str)
            .bind(pubkey_str.to_hex())
            .bind(recipient_str)
            .bind(withdrawal_hash)
            .bind(&proof_bytes)
            .bind(&claim_value)
            .bind(SqlWithdrawalStatus::Requested as SqlWithdrawalStatus)
            .execute(&server.pool)
            .await
            .expect("Failed to insert record into withdrawals table");

            let exists: (bool,) = sqlx::query_as::<_, (bool,)>(
                r#"
                SELECT EXISTS(
                    SELECT 1 FROM withdrawals WHERE withdrawal_hash = $1
                )
                "#,
            )
            .bind(withdrawal_hash)
            .fetch_one(&server.pool)
            .await
            .expect("Failed to check existence of withdrawal_hash in withdrawals table");

            assert_and_stop(cont_name, || {
                assert!(
                    exists.0,
                    "Withdrawals should contain withdrawal_hash after insertion"
                )
            });

            // Check claims table for some nullifier record
            let exists: (bool,) = sqlx::query_as::<_, (bool,)>(
                r#"
                SELECT EXISTS(
                    SELECT 1 FROM claims WHERE nullifier = $1
                )
                "#,
            )
            .bind(withdrawal_hash)
            .fetch_one(&server.pool)
            .await
            .expect("Failed to check existence of nullifier in claims table");

            assert_and_stop(cont_name, || {
                assert!(!exists.0, "Claim should not contain nullifier")
            });

            sqlx::query(
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
            )
            .bind(&uuid_str)
            .bind(pubkey_str.to_hex())
            .bind(recipient_str)
            .bind(withdrawal_hash)
            .bind(&proof_bytes)
            .bind(&claim_value)
            .bind(SqlClaimStatus::Requested as SqlClaimStatus)
            .execute(&server.pool)
            .await
            .expect("Failed to insert claim into database");

            let exists: (bool,) = sqlx::query_as::<_, (bool,)>(
                r#"
                SELECT EXISTS(
                    SELECT 1 FROM claims WHERE nullifier = $1
                )
                "#,
            )
            .bind(withdrawal_hash)
            .fetch_one(&server.pool)
            .await
            .expect("Failed to check existence of nullifier in claims table");

            assert_and_stop(cont_name, || {
                assert!(exists.0, "Claim should contain nullifier after insertion")
            });
        }

        stop_withdrawal_docker(cont_name);
    }
}

#[cfg(test)]
mod keyset_tests {
    use super::*;
    use ark_bn254::{Fr, G1Affine};
    use ark_ec::AffineRepr;
    use num_bigint::BigUint;
    use plonky2_bn254::fields::recover::RecoverFromX as _;

    fn assert_keyset_valid(h: B256) {
        let keyset = privkey_to_keyset(h);

        // Get expected pubkey from privkey
        let privkey_fr: Fr = BigUint::from(keyset.privkey).into();
        let expected_pubkey_g1: G1Affine = (G1Affine::generator() * privkey_fr).into();

        // Ensure pubkey is correct
        assert_eq!(
            keyset.pubkey_g1(),
            expected_pubkey_g1,
            "Public key mismatch for privkey: {:?}",
            h
        );

        // Ensure pubkey is not dummy
        assert!(
            !keyset.pubkey.is_dummy_pubkey(),
            "Pubkey should not be dummy: {:?}",
            keyset.pubkey
        );

        // Check recovery via x-coordinate
        let recovered = G1Affine::recover_from_x(keyset.pubkey.into());
        assert_eq!(
            recovered,
            keyset.pubkey_g1(),
            "Recovered pubkey from x doesn't match"
        );
    }

    #[test]
    #[should_panic]
    fn test_zero_privkey() {
        let h = B256::ZERO;
        assert_keyset_valid(h);
    }

    // It panics in KeySet::new, not in assert_keyset_valid
    #[test]
    #[should_panic(expected = "!pubkey.is_dummy_pubkey()")]
    fn test_one_privkey() {
        let mut bytes = [0u8; 32];
        bytes[31] = 0x01;
        let h = B256::from(bytes);
        assert_keyset_valid(h);
    }

    #[test]
    fn test_max_privkey() {
        let h = B256::from([0xFF; 32]);
        assert_keyset_valid(h);
    }

    #[test]
    fn test_near_max_privkey() {
        let mut bytes = [0xFF; 32];
        bytes[31] = 0xFE;
        let h = B256::from(bytes);
        assert_keyset_valid(h);
    }

    #[test]
    fn test_mid_privkey() {
        let mut bytes = [0u8; 32];
        bytes[0] = 0x80; // MSB = 1, rest = 0
        let h = B256::from(bytes);
        assert_keyset_valid(h);
    }

    #[test]
    fn test_leading_zeros_privkey() {
        let mut bytes = [0u8; 32];
        bytes[30] = 0x01;
        let h = B256::from(bytes);
        assert_keyset_valid(h);
    }
}
