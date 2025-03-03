use ethers::types::H256;
use intmax2_client_sdk::external_api::{
    contract::{
        block_builder_registry::BlockBuilderRegistryContract, rollup_contract::RollupContract,
    },
    store_vault_server::StoreVaultServerClient,
    validity_prover::ValidityProverClient,
};
use intmax2_interfaces::api::{
    block_builder::interface::{BlockBuilderFeeInfo, FeeProof},
    validity_prover::interface::ValidityProverClientInterface,
};
use intmax2_zkp::{
    common::{
        block_builder::{BlockProposal, UserSignature},
        tx::Tx,
    },
    ethereum_types::{u256::U256, u32limb_trait::U32LimbTrait},
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
    storage::{config::StorageConfig, redis_storage::RedisStorage, Storage},
};

pub const DEFAULT_POST_BLOCK_CHANNEL: u64 = 100;

#[derive(Debug, Clone)]
pub struct Config {
    pub block_builder_url: String,
    pub block_builder_private_key: H256,
    pub eth_allowance_for_block: U256,

    pub initial_heart_beat_delay: u64,
    pub heart_beat_interval: u64,

    // fees
    pub beneficiary_pubkey: Option<U256>,
    pub registration_fee: Option<HashMap<u32, U256>>,
    pub non_registration_fee: Option<HashMap<u32, U256>>,
    pub registration_collateral_fee: Option<HashMap<u32, U256>>,
    pub non_registration_collateral_fee: Option<HashMap<u32, U256>>,
}

#[derive(Clone)]
pub struct BlockBuilder {
    pub config: Config,
    pub store_vault_server_client: StoreVaultServerClient,
    pub validity_prover_client: ValidityProverClient,
    pub rollup_contract: RollupContract,
    pub registry_contract: BlockBuilderRegistryContract,

    pub storage: Arc<Box<dyn Storage>>,
}

impl BlockBuilder {
    pub async fn new(env: &EnvVar) -> Result<Self, BlockBuilderError> {
        let store_vault_server_client =
            StoreVaultServerClient::new(&env.store_vault_server_base_url);
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

        let eth_allowance_for_block = {
            let u = ethers::utils::parse_ether(env.eth_allowance_for_block.clone()).unwrap();
            let mut buf = [0u8; 32];
            u.to_big_endian(&mut buf);
            U256::from_bytes_be(&buf)
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

        let beneficiary_pubkey = env
            .beneficiary_pubkey
            .map(|pubkey| U256::from_bytes_be(pubkey.as_bytes()));

        let storage_config = StorageConfig {
            use_fee: registration_fee.is_some() || non_registration_fee.is_some(),
            use_collateral: registration_collateral_fee.is_some() || non_registration_fee.is_some(),
            fee_beneficiary: beneficiary_pubkey.unwrap_or_default(),
            tx_timeout: env.tx_timeout,
            accepting_tx_interval: env.accepting_tx_interval,
            proposing_block_interval: env.proposing_block_interval,
            deposit_check_interval: env.deposit_check_interval,
            redis_url: env.redis_url.clone(),
            block_builder_id: Uuid::new_v4().to_string(),
        };
        let redis_storage = RedisStorage::new(&storage_config).await;
        let storage: Arc<Box<dyn Storage>> = Arc::new(Box::new(redis_storage));

        let config = Config {
            block_builder_url: env.block_builder_url.clone(),
            block_builder_private_key: env.block_builder_private_key,
            eth_allowance_for_block,
            initial_heart_beat_delay: env.initial_heart_beat_delay,
            heart_beat_interval: env.heart_beat_interval,
            beneficiary_pubkey,
            registration_fee,
            non_registration_fee,
            registration_collateral_fee,
            non_registration_collateral_fee,
        };

        Ok(Self {
            config,
            store_vault_server_client,
            validity_prover_client,
            rollup_contract,
            registry_contract,
            storage,
        })
    }

    pub fn get_fee_info(&self) -> BlockBuilderFeeInfo {
        BlockBuilderFeeInfo {
            beneficiary: self.config.beneficiary_pubkey,
            registration_fee: convert_fee_vec(&self.config.registration_fee),
            non_registration_fee: convert_fee_vec(&self.config.non_registration_fee),
            registration_collateral_fee: convert_fee_vec(&self.config.registration_collateral_fee),
            non_registration_collateral_fee: convert_fee_vec(
                &self.config.non_registration_collateral_fee,
            ),
        }
    }

    // Send a tx request by the user.
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

        // registration check
        let account_info = self.validity_prover_client.get_account_info(pubkey).await?;
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

        // fee check
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
            &self.store_vault_server_client,
            self.config.beneficiary_pubkey,
            required_fee,
            required_collateral_fee,
            pubkey,
            fee_proof,
        )
        .await?;

        let request_id = Uuid::new_v4().to_string();
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

    // Query the constructed proposal by the user.
    pub async fn query_proposal(
        &self,
        request_id: &str,
    ) -> Result<Option<BlockProposal>, BlockBuilderError> {
        log::info!("query_proposal request_id: {}", request_id);
        let proposal = self.storage.query_proposal(request_id).await?;
        Ok(proposal)
    }

    // Post the signature by the user.
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
