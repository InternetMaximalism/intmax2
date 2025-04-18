use ethers::types::H256;
use intmax2_client_sdk::{
    client::key_from_eth::generate_intmax_account_from_eth_key,
    external_api::{
        contract::{
            block_builder_registry::BlockBuilderRegistryContract,
            rollup_contract::RollupContract,
            utils::{get_address, get_eth_balance},
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
    pub block_builder_private_key: H256,
    pub block_builder_address: Address,
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
    pub async fn new(env: &EnvVar) -> Result<Self, BlockBuilderError> {
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
        let rollup_contract = RollupContract::new(
            &env.l2_rpc_url,
            env.l2_chain_id,
            env.rollup_contract_address,
            env.rollup_contract_deployed_block_number,
        );
        let registry_contract = BlockBuilderRegistryContract::new(
            &env.l2_rpc_url,
            env.l2_chain_id,
            env.block_builder_registry_contract_address,
        );
        let config = Self::create_config(env)?;
        let storage = Self::create_storage(env, &config).await?;

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
            let u = ethers::utils::parse_ether(env.eth_allowance_for_block.clone()).unwrap();
            convert_u256_from_ether_to_intmax(u)
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
            registration_collateral_fee.is_some() || non_registration_fee.is_some();
        if use_collateral_fee && !use_fee {
            return Err(BlockBuilderError::InvalidFeeSetting(
                "Collateral fee is set but fee is not set".to_string(),
            ));
        }
        let beneficiary_pubkey = if use_fee {
            if let Some(beneficiary_pubkey) = env.beneficiary_pubkey.as_ref() {
                Some(U256::from_bytes_be(beneficiary_pubkey.as_bytes()).unwrap())
            } else {
                // generate from eth private key
                let key = generate_intmax_account_from_eth_key(env.block_builder_private_key);
                Some(key.pubkey)
            }
        } else {
            None
        };
        let block_builder_address = Address::from_bytes_be(
            get_address(env.l2_chain_id, env.block_builder_private_key).as_bytes(),
        )
        .unwrap();

        // log configuration
        log::info!("eth_allowance_for_block: {}", eth_allowance_for_block);
        log::info!("use_fee: {}", use_fee);
        log::info!("use_collateral_fee: {}", use_collateral_fee);
        log::info!(
            "beneficiary_pubkey: {}",
            beneficiary_pubkey.map(|b| b.to_hex()).unwrap_or_default()
        );
        let config = Config {
            block_builder_url: env.block_builder_url.clone(),
            block_builder_private_key: env.block_builder_private_key,
            block_builder_address,
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
            redis_url: env.redis_url.clone(),
            cluster_id: env.cluster_id.clone(),
            block_builder_id: Uuid::new_v4().to_string(),
        };
        let storage = storage::create_storage(&storage_config).await;
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
        let rpc_url = self.registry_contract.rpc_url.clone();
        let block_builder_address =
            ethers::types::Address::from_slice(&self.config.block_builder_address.to_bytes_be());
        let balance = get_eth_balance(&rpc_url, block_builder_address)
            .await
            .map_err(|e| {
                BlockBuilderError::BlockChainHealthError(format!(
                    "Failed to get block builder's balance: {}",
                    e
                ))
            })?;
        let balance = convert_u256_from_ether_to_intmax(balance);
        log::info!("block builder balance: {}", balance);
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
        log::info!(
            "send_tx_request is_registration_block: {}",
            is_registration_block
        );
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
        log::info!("query_proposal request_id: {}", request_id);
        let proposal = self.storage.query_proposal(request_id).await?;
        Ok(proposal)
    }

    /// Post the signature by the user
    pub async fn post_signature(
        &self,
        request_id: &str,
        signature: UserSignature,
    ) -> Result<(), BlockBuilderError> {
        log::info!("post_signature request_id: {}", request_id);
        self.storage.add_signature(request_id, signature).await?;
        Ok(())
    }
}

fn convert_u256_from_ether_to_intmax(
    u: ethers::types::U256,
) -> intmax2_zkp::ethereum_types::u256::U256 {
    let mut buf = [0u8; 32];
    u.to_big_endian(&mut buf);
    intmax2_zkp::ethereum_types::u256::U256::from_bytes_be(&buf).unwrap()
}
