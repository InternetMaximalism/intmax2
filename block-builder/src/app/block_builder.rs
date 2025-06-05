use alloy::{
    primitives::{utils::parse_ether, B256},
    providers::Provider,
};
use intmax2_client_sdk::{
    client::key_from_eth::generate_intmax_account_from_eth_key,
    external_api::{
        contract::{
            block_builder_registry::BlockBuilderRegistryContract,
            convert::{
                convert_address_to_alloy, convert_address_to_intmax, convert_u256_to_intmax,
            },
            rollup_contract::RollupContract,
            utils::{get_address_from_private_key, NormalProvider},
        },
        s3_store_vault::S3StoreVaultClient,
        store_vault_server::StoreVaultServerClient,
        validity_prover::ValidityProverClient,
    },
};
use intmax2_interfaces::api::{
    block_builder::interface::{BlockBuilderFeeInfo, FeeProof},
    store_vault_server::interface::StoreVaultClientInterface,
    validity_prover::interface::{AccountInfo, ValidityProverClientInterface},
};
use intmax2_zkp::{
    common::{
        block_builder::{BlockProposal, UserSignature},
        tx::Tx,
    },
    ethereum_types::{
        account_id::AccountId, address::Address, u256::U256, u32limb_trait::U32LimbTrait,
    },
};
use std::{collections::HashMap, sync::Arc};
use uuid::Uuid;

use crate::{
    app::{fee::validate_fee_proof, types::TxRequest},
    EnvVar,
};

use super::{
    error::BlockBuilderError,
    fee::{convert_fee_vec, parse_fee_str},
    storage::{self, config::StorageConfig, Storage},
};

pub const DEFAULT_POST_BLOCK_CHANNEL: u64 = 100;

#[derive(Debug, Clone)]
pub struct Config {
    pub block_builder_url: String,
    pub block_builder_private_key: B256,
    pub block_builder_address: Address,
    pub gas_limit_for_block_post: Option<u64>,
    pub eth_allowance_for_block: U256,

    pub initial_heart_beat_delay: u64,
    pub heart_beat_interval: u64,

    // fees
    pub beneficiary_pubkey: Option<U256>,
    pub use_fee: bool,
    pub use_collateral: bool,
    pub registration_fee: Option<HashMap<u32, U256>>,
    pub non_registration_fee: Option<HashMap<u32, U256>>,
    pub registration_collateral_fee: Option<HashMap<u32, U256>>,
    pub non_registration_collateral_fee: Option<HashMap<u32, U256>>,
}

#[derive(Clone)]
pub struct BlockBuilder {
    pub config: Config,
    pub store_vault_server_client: Arc<Box<dyn StoreVaultClientInterface>>,
    pub validity_prover_client: ValidityProverClient,
    pub rollup_contract: RollupContract,
    pub registry_contract: BlockBuilderRegistryContract,

    pub storage: Arc<Box<dyn Storage>>,
}

impl BlockBuilder {
    /// Create a new BlockBuilder instance
    pub async fn new(env: &EnvVar, provider: NormalProvider) -> Result<Self, BlockBuilderError> {
        // Initialize clients
        let store_vault_server_client: Arc<Box<dyn StoreVaultClientInterface>> =
            if env.use_s3.unwrap_or(true) {
                log::info!("Using s3_store_vault");
                Arc::new(Box::new(S3StoreVaultClient::new(
                    &env.store_vault_server_base_url,
                )))
            } else {
                log::info!("Using store_vault_server");
                Arc::new(Box::new(StoreVaultServerClient::new(
                    &env.store_vault_server_base_url,
                )))
            };
        let validity_prover_client = ValidityProverClient::new(&env.validity_prover_base_url);
        let rollup_contract = RollupContract::new(provider.clone(), env.rollup_contract_address);
        let registry_contract = BlockBuilderRegistryContract::new(
            provider,
            env.block_builder_registry_contract_address,
        );
        let config = Self::create_config(env)?;
        let storage = Self::create_storage(env, &config, rollup_contract.clone()).await?;

        Ok(Self {
            config,
            store_vault_server_client,
            validity_prover_client,
            rollup_contract,
            registry_contract,
            storage,
        })
    }

    /// Create configuration from environment variables
    fn create_config(env: &EnvVar) -> Result<Config, BlockBuilderError> {
        let eth_allowance_for_block = {
            let u = parse_ether(&env.eth_allowance_for_block).unwrap();
            convert_u256_to_intmax(u)
        };
        let registration_fee = env
            .registration_fee
            .as_ref()
            .map(|fee| parse_fee_str(fee))
            .transpose()?;
        let non_registration_fee = env
            .non_registration_fee
            .as_ref()
            .map(|fee| parse_fee_str(fee))
            .transpose()?;
        let registration_collateral_fee = env
            .registration_collateral_fee
            .as_ref()
            .map(|fee| parse_fee_str(fee))
            .transpose()?;
        let non_registration_collateral_fee = env
            .non_registration_collateral_fee
            .as_ref()
            .map(|fee| parse_fee_str(fee))
            .transpose()?;
        let use_fee = registration_fee.is_some() || non_registration_fee.is_some();
        let use_collateral_fee =
            registration_collateral_fee.is_some() || non_registration_collateral_fee.is_some();
        if use_collateral_fee && !use_fee {
            return Err(BlockBuilderError::InvalidFeeSetting(
                "Collateral fee is set but fee is not set".to_string(),
            ));
        }
        let beneficiary_pubkey = if use_fee {
            if let Some(beneficiary_pubkey) = env.beneficiary_pubkey.as_ref() {
                Some((*beneficiary_pubkey).into())
            } else {
                // generate from eth private key
                let key = generate_intmax_account_from_eth_key(env.block_builder_private_key);
                Some(key.pubkey)
            }
        } else {
            None
        };
        let block_builder_address =
            convert_address_to_intmax(get_address_from_private_key(env.block_builder_private_key));
        // log configuration
        log::info!("block_builder_address: {block_builder_address}");
        log::info!("block_builder_url: {}", env.block_builder_url);
        log::info!(
            "gas limit for block post: {:?}",
            env.gas_limit_for_block_post.clone()
        );
        log::info!("eth_allowance_for_block: {eth_allowance_for_block}");
        log::info!("use_fee: {use_fee}");
        log::info!("use_collateral_fee: {use_collateral_fee}");
        log::info!(
            "beneficiary_pubkey: {}",
            beneficiary_pubkey.map(|b| b.to_hex()).unwrap_or_default()
        );
        let config = Config {
            block_builder_url: env.block_builder_url.clone(),
            block_builder_private_key: env.block_builder_private_key,
            block_builder_address,
            gas_limit_for_block_post: env.gas_limit_for_block_post,
            eth_allowance_for_block,
            initial_heart_beat_delay: env.initial_heart_beat_delay,
            heart_beat_interval: env.heart_beat_interval,
            beneficiary_pubkey,
            use_fee,
            use_collateral: use_collateral_fee,
            registration_fee,
            non_registration_fee,
            registration_collateral_fee,
            non_registration_collateral_fee,
        };
        Ok(config)
    }

    /// Create storage based on configuration
    async fn create_storage(
        env: &EnvVar,
        config: &Config,
        rollup: RollupContract,
    ) -> Result<Arc<Box<dyn Storage>>, BlockBuilderError> {
        let storage_config = StorageConfig {
            use_fee: config.use_fee,
            use_collateral: config.use_collateral,
            block_builder_address: config.block_builder_address,
            fee_beneficiary: config.beneficiary_pubkey.unwrap_or_default(),
            tx_timeout: env.tx_timeout,
            accepting_tx_interval: env.accepting_tx_interval,
            proposing_block_interval: env.proposing_block_interval,
            deposit_check_interval: env.deposit_check_interval,
            nonce_waiting_time: env.nonce_waiting_time.unwrap_or(5),
            redis_url: env.redis_url.clone(),
            cluster_id: env.cluster_id.clone(),
            block_builder_id: Uuid::new_v4().to_string(),
        };
        let storage = storage::create_storage(&storage_config, rollup).await;
        Ok(Arc::new(storage))
    }

    /// Get fee information for the block builder
    pub fn get_fee_info(&self) -> BlockBuilderFeeInfo {
        BlockBuilderFeeInfo {
            block_builder_address: self.config.block_builder_address,
            beneficiary: self.config.beneficiary_pubkey,
            registration_fee: convert_fee_vec(&self.config.registration_fee),
            non_registration_fee: convert_fee_vec(&self.config.non_registration_fee),
            registration_collateral_fee: convert_fee_vec(&self.config.registration_collateral_fee),
            non_registration_collateral_fee: convert_fee_vec(
                &self.config.non_registration_collateral_fee,
            ),
        }
    }

    /// Check RPC connection and block builder's balance
    pub async fn blockchain_health_check(&self) -> Result<(), BlockBuilderError> {
        log::info!("check_balance");
        let block_builder_address = convert_address_to_alloy(self.config.block_builder_address);
        let balance = self
            .registry_contract
            .provider
            .get_balance(block_builder_address)
            .await
            .map_err(|e| BlockBuilderError::BlockChainHealthError(e.to_string()))?;
        let balance = convert_u256_to_intmax(balance);
        log::info!("block builder balance: {balance}");
        if balance < self.config.eth_allowance_for_block {
            return Err(BlockBuilderError::BlockChainHealthError(format!(
                "Block builder's balance is not enough: current {} < required {}",
                balance, self.config.eth_allowance_for_block
            )));
        }
        Ok(())
    }

    /// Send a transaction request by the user
    pub async fn send_tx_request(
        &self,
        is_registration_block: bool,
        pubkey: U256,
        tx: Tx,
        fee_proof: &Option<FeeProof>,
    ) -> Result<String, BlockBuilderError> {
        log::info!("send_tx_request is_registration_block: {is_registration_block}");
        // Verify account info
        let account_info = self.validity_prover_client.get_account_info(pubkey).await?;
        self.verify_account_info(is_registration_block, pubkey, &account_info)
            .await?;

        // Verify fee proof
        self.verify_fee_proof(is_registration_block, pubkey, fee_proof)
            .await?;

        // Create and add transaction request
        let request_id = Uuid::new_v4().to_string();
        let account_id = account_info.account_id.map(AccountId);
        let tx_request = TxRequest {
            pubkey,
            account_id,
            tx,
            fee_proof: fee_proof.clone(),
            request_id: request_id.clone(),
        };
        self.storage
            .add_tx(is_registration_block, tx_request)
            .await?;
        Ok(request_id)
    }

    /// Verify account status for a transaction
    async fn verify_account_info(
        &self,
        is_registration_block: bool,
        pubkey: U256,
        account_info: &AccountInfo,
    ) -> Result<(), BlockBuilderError> {
        let account_id = account_info.account_id;
        if is_registration_block {
            if let Some(account_id) = account_id {
                return Err(BlockBuilderError::AccountAlreadyRegistered(
                    pubkey, account_id,
                ));
            }
        } else if account_id.is_none() {
            return Err(BlockBuilderError::AccountNotFound(pubkey));
        }
        Ok(())
    }

    /// Verify fee proof for a transaction
    async fn verify_fee_proof(
        &self,
        is_registration_block: bool,
        pubkey: U256,
        fee_proof: &Option<FeeProof>,
    ) -> Result<(), BlockBuilderError> {
        let required_fee = if is_registration_block {
            self.config.registration_fee.as_ref()
        } else {
            self.config.non_registration_fee.as_ref()
        };

        let required_collateral_fee = if is_registration_block {
            self.config.registration_collateral_fee.as_ref()
        } else {
            self.config.non_registration_collateral_fee.as_ref()
        };

        validate_fee_proof(
            self.store_vault_server_client.as_ref().as_ref(),
            self.config.beneficiary_pubkey,
            self.config.block_builder_address,
            required_fee,
            required_collateral_fee,
            pubkey,
            fee_proof,
        )
        .await
        .map_err(BlockBuilderError::FeeError)
    }

    /// Query the constructed proposal by the user
    pub async fn query_proposal(
        &self,
        request_id: &str,
    ) -> Result<Option<BlockProposal>, BlockBuilderError> {
        log::info!("query_proposal request_id: {request_id}");
        let proposal = self.storage.query_proposal(request_id).await?;
        Ok(proposal)
    }

    /// Post the signature by the user
    pub async fn post_signature(
        &self,
        request_id: &str,
        signature: UserSignature,
    ) -> Result<(), BlockBuilderError> {
        log::info!("post_signature request_id: {request_id}");
        self.storage.add_signature(request_id, signature).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use alloy::{
        primitives::Address as AlloyAddress,
        providers::{mock::Asserter, ProviderBuilder},
    };

    use crate::app::storage::redis_storage::test_redis_helper::{
        find_free_port, run_redis_docker, stop_redis_docker,
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

    #[tokio::test]
    async fn test_get_fee_info() {
        // Initialize our own EnvVar
        let env = EnvVar {
            port: 9004,
            block_builder_url: "http://localhost:9004".to_string(),
            redis_url: None,
            cluster_id: Some("1".to_string()),
            l2_rpc_url: "http://localhost:8545".to_string(),
            rollup_contract_address: AlloyAddress::default(),
            block_builder_registry_contract_address: AlloyAddress::default(),
            store_vault_server_base_url: "http://localhost:9000".to_string(),
            use_s3: Some(false),
            validity_prover_base_url: "http://localhost:9100".to_string(),
            block_builder_private_key:
                "0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d"
                    .parse()
                    .unwrap(), // anvil key
            eth_allowance_for_block: "0.3".to_string(),
            tx_timeout: 80,
            accepting_tx_interval: 40,
            proposing_block_interval: 10,
            deposit_check_interval: Some(20),
            initial_heart_beat_delay: 600,
            gas_limit_for_block_post: Some(40000),
            heart_beat_interval: 86400,
            nonce_waiting_time: None,
            beneficiary_pubkey: None,
            registration_fee: Some("0:100,1:2000".to_string()),
            non_registration_fee: Some("0:100,1:2000".to_string()),
            registration_collateral_fee: None,
            non_registration_collateral_fee: None,
        };

        let block_builder = BlockBuilder::new(&env, get_provider()).await.unwrap();
        let info = block_builder.get_fee_info();

        assert_eq!(
            info.block_builder_address,
            block_builder.config.block_builder_address
        );
        assert_eq!(info.beneficiary, block_builder.config.beneficiary_pubkey);
        assert_eq!(
            info.registration_fee,
            convert_fee_vec(&block_builder.config.registration_fee)
        );
        assert_eq!(
            info.non_registration_fee,
            convert_fee_vec(&block_builder.config.non_registration_fee)
        );
    }

    #[tokio::test]
    async fn test_blockchain_health_check_not_enough_balance() {
        // Initialize our own EnvVar
        let env = EnvVar {
            port: 9004,
            block_builder_url: "http://localhost:9004".to_string(),
            redis_url: None,
            cluster_id: Some("1".to_string()),
            l2_rpc_url: "http://localhost:8545".to_string(),
            rollup_contract_address: AlloyAddress::default(),
            block_builder_registry_contract_address: AlloyAddress::default(),
            store_vault_server_base_url: "http://localhost:9000".to_string(),
            use_s3: Some(false),
            validity_prover_base_url: "http://localhost:9100".to_string(),
            block_builder_private_key:
                "0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d"
                    .parse()
                    .unwrap(), // anvil key
            eth_allowance_for_block: "0.001".to_string(), // this value is important in this test
            tx_timeout: 80,
            accepting_tx_interval: 40,
            proposing_block_interval: 10,
            deposit_check_interval: Some(20),
            initial_heart_beat_delay: 600,
            gas_limit_for_block_post: Some(40000),
            heart_beat_interval: 86400,
            nonce_waiting_time: None,
            beneficiary_pubkey: None,
            registration_fee: Some("0:100,1:2000".to_string()),
            non_registration_fee: Some("0:100,1:2000".to_string()),
            registration_collateral_fee: None,
            non_registration_collateral_fee: None,
        };

        let block_builder = BlockBuilder::new(&env, get_provider()).await.unwrap();
        let result = block_builder.blockchain_health_check().await;

        assert!(matches!(
            result,
            Err(BlockBuilderError::BlockChainHealthError(_))
        ));
    }

    #[tokio::test]
    async fn test_blockchain_health_check_ok() {
        // Initialize our own EnvVar
        let env = EnvVar {
            port: 9004,
            block_builder_url: "http://localhost:9004".to_string(),
            redis_url: None,
            cluster_id: Some("1".to_string()),
            l2_rpc_url: "http://localhost:8545".to_string(),
            rollup_contract_address: AlloyAddress::default(),
            block_builder_registry_contract_address: AlloyAddress::default(),
            store_vault_server_base_url: "http://localhost:9000".to_string(),
            use_s3: Some(false),
            validity_prover_base_url: "http://localhost:9100".to_string(),
            block_builder_private_key:
                "0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d"
                    .parse()
                    .unwrap(), // anvil key
            eth_allowance_for_block: "1000000000".to_string(), // this value is important in this test
            tx_timeout: 80,
            accepting_tx_interval: 40,
            proposing_block_interval: 10,
            deposit_check_interval: Some(20),
            initial_heart_beat_delay: 600,
            gas_limit_for_block_post: Some(40000),
            heart_beat_interval: 86400,
            nonce_waiting_time: None,
            beneficiary_pubkey: None,
            registration_fee: Some("0:100,1:2000".to_string()),
            non_registration_fee: Some("0:100,1:2000".to_string()),
            registration_collateral_fee: None,
            non_registration_collateral_fee: None,
        };

        let block_builder = BlockBuilder::new(&env, get_provider()).await.unwrap();
        let result = block_builder.blockchain_health_check().await;

        assert!(matches!(
            result,
            Err(BlockBuilderError::BlockChainHealthError(_))
        ));
    }

    #[tokio::test]
    async fn test_creating_with_redis() {
        let port = find_free_port();
        let cont_name = "block-builder-test-creating-with-redis";

        // Initialize our own EnvVar
        let env = EnvVar {
            port: 9004,
            block_builder_url: "http://localhost:9004".to_string(),
            redis_url: Some(format!("redis://localhost:{port}").to_string()),
            cluster_id: Some("1".to_string()),
            l2_rpc_url: "http://localhost:8545".to_string(),
            rollup_contract_address: AlloyAddress::default(),
            block_builder_registry_contract_address: AlloyAddress::default(),
            store_vault_server_base_url: "http://localhost:9000".to_string(),
            use_s3: Some(false),
            validity_prover_base_url: "http://localhost:9100".to_string(),
            block_builder_private_key:
                "0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d"
                    .parse()
                    .unwrap(), // anvil key
            eth_allowance_for_block: "0.3".to_string(),
            tx_timeout: 80,
            accepting_tx_interval: 40,
            proposing_block_interval: 10,
            deposit_check_interval: Some(20),
            initial_heart_beat_delay: 600,
            gas_limit_for_block_post: Some(40000),
            heart_beat_interval: 86400,
            nonce_waiting_time: None,
            beneficiary_pubkey: None,
            registration_fee: Some("0:100,1:2000".to_string()),
            non_registration_fee: Some("0:100,1:2000".to_string()),
            registration_collateral_fee: None,
            non_registration_collateral_fee: None,
        };

        // Run docker image
        stop_redis_docker(cont_name);
        let output = run_redis_docker(port, cont_name);
        assert!(
            output.status.success(),
            "Couldn't start {}: {}",
            cont_name,
            String::from_utf8_lossy(&output.stderr)
        );

        let _ = BlockBuilder::new(&env, get_provider()).await.unwrap();

        // Stop docker image
        let output = stop_redis_docker(cont_name);
        assert!(
            output.status.success(),
            "Couldn't stop {}: {}",
            cont_name,
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
