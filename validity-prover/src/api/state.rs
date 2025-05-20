use futures::future;
use intmax2_client_sdk::external_api::contract::{
    liquidity_contract::LiquidityContract, rollup_contract::RollupContract,
    utils::get_provider_with_fallback,
};
use intmax2_interfaces::{
    api::validity_prover::{
        interface::DepositInfo,
        types::{
            GetAccountInfoBatchRequest, GetAccountInfoBatchResponse, GetAccountInfoQuery,
            GetAccountInfoResponse, GetBlockMerkleProofQuery, GetBlockMerkleProofResponse,
            GetBlockNumberByTxTreeRootBatchRequest, GetBlockNumberByTxTreeRootBatchResponse,
            GetBlockNumberByTxTreeRootQuery, GetBlockNumberByTxTreeRootResponse,
            GetDepositInfoBatchRequest, GetDepositInfoBatchResponse, GetDepositInfoQuery,
            GetDepositInfoResponse, GetDepositMerkleProofQuery, GetDepositMerkleProofResponse,
            GetUpdateWitnessQuery, GetUpdateWitnessResponse, GetValidityProofQuery,
            GetValidityProofResponse, GetValidityWitnessQuery, GetValidityWitnessResponse,
        },
    },
    data::proof_compression::CompressedValidityProof,
};
use intmax2_zkp::common::{
    trees::{block_hash_tree::BlockHashMerkleProof, deposit_tree::DepositMerkleProof},
    witness::validity_witness::ValidityWitness,
};
use std::{sync::Arc, time::Duration};
use tracing::info;

use server_common::{parser::parse_urls, redis::cache::RedisCache};

use crate::{
    app::{
        leader_election::LeaderElection, observer_api::ObserverApi,
        observer_common::run_and_switch_observers, observer_graph::TheGraphObserver,
        observer_rpc::RPCObserver, rate_manager::RateManager, validity_prover::ValidityProver,
    },
    EnvVar,
};

pub struct HealthCheckConfig {
    pub thread_heartbeat_timeout: Duration,
}

pub struct CacheConfig {
    pub dynamic_ttl: Duration,
    pub static_ttl: Duration,
}

/// The state of the server.
/// Added cache layer to the state.
pub struct State {
    pub validity_prover: ValidityProver,
    pub rate_manager: RateManager,
    pub cache: RedisCache,
    pub cache_config: CacheConfig,
    pub health_check_config: HealthCheckConfig,
}

impl State {
    pub async fn new(env: &EnvVar) -> anyhow::Result<Self> {
        let l1_rpc_urls = parse_urls(&env.l1_rpc_url)?;
        let l2_rpc_urls = parse_urls(&env.l2_rpc_url)?;
        let l1_provider = get_provider_with_fallback(l1_rpc_urls.as_ref())?;
        let l2_provider = get_provider_with_fallback(l2_rpc_urls.as_ref())?;
        let rollup_contract = RollupContract::new(l2_provider, env.rollup_contract_address);
        let liquidity_contract =
            LiquidityContract::new(l1_provider, env.liquidity_contract_address);
        let observer_api = ObserverApi::new(env, rollup_contract, liquidity_contract).await?;
        let leader_election = LeaderElection::new(
            &env.redis_url,
            "validity_prover:sync_leader",
            std::time::Duration::from_secs(env.leader_lock_ttl),
        )?;
        let rate_manager = RateManager::new(
            Duration::from_secs(env.rate_manager_window),
            Duration::from_secs(env.rate_manager_timeout),
        );

        let validity_prover = ValidityProver::new(
            env,
            observer_api.clone(),
            leader_election.clone(),
            rate_manager.clone(),
        )
        .await?;
        let cache = RedisCache::new(&env.redis_url, "validity_prover:cache")?;
        let cache_config = CacheConfig {
            dynamic_ttl: Duration::from_secs(env.dynamic_cache_ttl),
            static_ttl: Duration::from_secs(env.static_cache_ttl),
        };
        let health_check_config = HealthCheckConfig {
            thread_heartbeat_timeout: Duration::from_secs(env.thread_heartbeat_timeout),
        };

        let rpc_observer = RPCObserver::new(
            env,
            observer_api.clone(),
            leader_election.clone(),
            rate_manager.clone(),
        )
        .await?;
        let graph_observer = if env.the_graph_l1_url.is_some() && env.the_graph_l2_url.is_some() {
            log::info!("The Graph observer is enabled");
            Some(Arc::new(
                TheGraphObserver::new(
                    env,
                    observer_api,
                    leader_election.clone(),
                    rate_manager.clone(),
                )
                .await?,
            ))
        } else {
            None
        };

        // start jos
        if env.is_sync_mode {
            leader_election.start_job();
            run_and_switch_observers(Arc::new(rpc_observer), graph_observer).await;
            validity_prover.start_all_jobs().await?;
            info!("Started all jobs");
        }

        Ok(Self {
            validity_prover,
            rate_manager,
            cache,
            cache_config,
            health_check_config,
        })
    }

    pub async fn get_block_number(&self) -> anyhow::Result<u32> {
        type V = u32;
        let key = "block_number";
        if let Some(block_number) = self.cache.get::<V>(key).await? {
            Ok(block_number)
        } else {
            let block_number = self.validity_prover.get_last_block_number().await?;
            self.cache
                .set_with_ttl::<V>(key, &block_number, self.cache_config.dynamic_ttl)
                .await?;
            Ok(block_number)
        }
    }

    pub async fn get_validity_proof_block_number(&self) -> anyhow::Result<u32> {
        type V = u32;
        let key = "validity_proof_block_number";
        if let Some(block_number) = self.cache.get::<V>(key).await? {
            Ok(block_number)
        } else {
            let block_number = self
                .validity_prover
                .get_latest_validity_proof_block_number()
                .await?;
            self.cache
                .set_with_ttl::<V>(key, &block_number, self.cache_config.dynamic_ttl)
                .await?;
            Ok(block_number)
        }
    }

    pub async fn get_next_deposit_index(&self) -> anyhow::Result<u32> {
        type V = u32;
        let key = "next_deposit_index";
        if let Some(deposit_index) = self.cache.get::<V>(key).await? {
            Ok(deposit_index)
        } else {
            let deposit_index = self
                .validity_prover
                .observer_api
                .get_next_deposit_index()
                .await?;
            self.cache
                .set_with_ttl::<V>(key, &deposit_index, self.cache_config.dynamic_ttl)
                .await?;
            Ok(deposit_index)
        }
    }

    pub async fn get_last_deposit_id(&self) -> anyhow::Result<u64> {
        type V = u64;
        let key = "last_deposit_id";
        if let Some(deposit_id) = self.cache.get::<V>(key).await? {
            Ok(deposit_id)
        } else {
            let deposit_id = self
                .validity_prover
                .observer_api
                .get_local_last_deposit_id()
                .await?;
            self.cache
                .set_with_ttl::<V>(key, &deposit_id, self.cache_config.dynamic_ttl)
                .await?;
            Ok(deposit_id)
        }
    }

    pub async fn get_latest_included_deposit_index(&self) -> anyhow::Result<Option<u32>> {
        type V = Option<u32>;
        let key = "latest_included_deposit_index";
        if let Some(deposit_index) = self.cache.get::<V>(key).await? {
            Ok(deposit_index)
        } else {
            let deposit_index = self
                .validity_prover
                .observer_api
                .get_latest_included_deposit_index()
                .await?;
            self.cache
                .set_with_ttl::<V>(key, &deposit_index, self.cache_config.dynamic_ttl)
                .await?;
            Ok(deposit_index)
        }
    }

    pub async fn get_account_info(
        &self,
        request: GetAccountInfoQuery,
    ) -> anyhow::Result<GetAccountInfoResponse> {
        // should not use cache for account info because it is mutable
        // and very likely to be updated
        let account_info = self
            .validity_prover
            .get_account_info(request.pubkey)
            .await?;
        Ok(GetAccountInfoResponse { account_info })
    }

    pub async fn get_account_info_batch(
        &self,
        request: &GetAccountInfoBatchRequest,
    ) -> anyhow::Result<GetAccountInfoBatchResponse> {
        // should not use cache for account info because it is mutable
        // and very likely to be updated
        let account_info = self
            .validity_prover
            .get_account_info_batch(&request.pubkeys)
            .await?;

        Ok(GetAccountInfoBatchResponse { account_info })
    }

    pub async fn get_update_witness(
        &self,
        request: GetUpdateWitnessQuery,
    ) -> anyhow::Result<GetUpdateWitnessResponse> {
        let key = format!("get_update_witness:{}", serde_qs::to_string(&request)?);
        if let Some(update_witness) = self.cache.get(&key).await? {
            Ok(GetUpdateWitnessResponse { update_witness })
        } else {
            let update_witness = self
                .validity_prover
                .get_update_witness(
                    request.pubkey,
                    request.root_block_number,
                    request.leaf_block_number,
                    request.is_prev_account_tree,
                )
                .await?;
            self.cache
                .set_with_ttl(&key, &update_witness, self.cache_config.static_ttl)
                .await?;
            Ok(GetUpdateWitnessResponse { update_witness })
        }
    }

    pub async fn get_validity_witness(
        &self,
        request: GetValidityWitnessQuery,
    ) -> anyhow::Result<GetValidityWitnessResponse> {
        type V = ValidityWitness;
        let key = format!("get_validity_witness:{}", serde_qs::to_string(&request)?);
        if let Some(validity_witness) = self.cache.get::<V>(&key).await? {
            Ok(GetValidityWitnessResponse { validity_witness })
        } else {
            let validity_witness = self
                .validity_prover
                .get_validity_witness(request.block_number)
                .await?;
            self.cache
                .set_with_ttl::<V>(&key, &validity_witness, self.cache_config.static_ttl)
                .await?;
            Ok(GetValidityWitnessResponse { validity_witness })
        }
    }

    pub async fn get_validity_proof(
        &self,
        request: GetValidityProofQuery,
    ) -> anyhow::Result<GetValidityProofResponse> {
        type V = CompressedValidityProof;
        let key = format!("get_validity_proof:{}", serde_qs::to_string(&request)?);
        if let Some(validity_proof) = self.cache.get::<V>(&key).await? {
            Ok(GetValidityProofResponse { validity_proof })
        } else {
            let validity_proof_raw = self
                .validity_prover
                .get_validity_proof(request.block_number)
                .await?
                .ok_or(anyhow::anyhow!(
                    "No validity proof found for block number {}",
                    request.block_number
                ))?;
            let validity_proof = CompressedValidityProof::new(&validity_proof_raw)?;
            self.cache
                .set_with_ttl::<V>(&key, &validity_proof, self.cache_config.static_ttl)
                .await?;
            Ok(GetValidityProofResponse { validity_proof })
        }
    }

    pub async fn get_deposit_info(
        &self,
        request: GetDepositInfoQuery,
    ) -> anyhow::Result<GetDepositInfoResponse> {
        type V = Option<DepositInfo>;
        let key = format!("get_deposit_info:{}", serde_qs::to_string(&request)?);
        if let Some(deposit_info) = self.cache.get::<V>(&key).await? {
            Ok(GetDepositInfoResponse { deposit_info })
        } else {
            let deposit_info = self
                .validity_prover
                .observer_api
                .get_deposit_info(request.pubkey_salt_hash)
                .await?;
            // the result is mutable
            self.cache
                .set_with_ttl::<V>(&key, &deposit_info, self.cache_config.dynamic_ttl)
                .await?;
            Ok(GetDepositInfoResponse { deposit_info })
        }
    }

    pub async fn get_deposit_info_batch(
        &self,
        request: &GetDepositInfoBatchRequest,
    ) -> anyhow::Result<GetDepositInfoBatchResponse> {
        // should use batch query instead
        let mut futures = Vec::with_capacity(request.pubkey_salt_hashes.len());
        for &pubkey_salt_hash in &request.pubkey_salt_hashes {
            let query = GetDepositInfoQuery { pubkey_salt_hash };
            let future = async move { self.get_deposit_info(query).await };
            futures.push(future);
        }
        let responses = future::join_all(futures)
            .await
            .into_iter()
            .collect::<anyhow::Result<Vec<_>>>()?;
        let deposit_info = responses.into_iter().map(|r| r.deposit_info).collect();
        Ok(GetDepositInfoBatchResponse { deposit_info })
    }

    pub async fn get_block_number_by_tx_tree_root(
        &self,
        request: GetBlockNumberByTxTreeRootQuery,
    ) -> anyhow::Result<GetBlockNumberByTxTreeRootResponse> {
        type V = Option<u32>;
        let key = format!(
            "get_block_number_by_tx_tree_root:{}",
            serde_qs::to_string(&request)?
        );
        if let Some(block_number) = self.cache.get::<V>(&key).await? {
            Ok(GetBlockNumberByTxTreeRootResponse { block_number })
        } else {
            let block_number = self
                .validity_prover
                .get_block_number_by_tx_tree_root(request.tx_tree_root)
                .await?;
            // the result is mutable
            self.cache
                .set_with_ttl::<V>(&key, &block_number, self.cache_config.dynamic_ttl)
                .await?;
            Ok(GetBlockNumberByTxTreeRootResponse { block_number })
        }
    }

    pub async fn get_block_number_by_tx_tree_root_batch(
        &self,
        request: &GetBlockNumberByTxTreeRootBatchRequest,
    ) -> anyhow::Result<GetBlockNumberByTxTreeRootBatchResponse> {
        // should not use cache because the combination is too many and
        // we should use batch query instead
        let block_numbers = self
            .validity_prover
            .get_block_number_by_tx_tree_root_batch(&request.tx_tree_roots)
            .await?;
        Ok(GetBlockNumberByTxTreeRootBatchResponse { block_numbers })
    }

    pub async fn get_block_merkle_proof(
        &self,
        request: GetBlockMerkleProofQuery,
    ) -> anyhow::Result<GetBlockMerkleProofResponse> {
        type V = BlockHashMerkleProof;
        let key = format!("get_block_merkle_proof:{}", serde_qs::to_string(&request)?);
        if let Some(block_merkle_proof) = self.cache.get::<V>(&key).await? {
            Ok(GetBlockMerkleProofResponse { block_merkle_proof })
        } else {
            let block_merkle_proof = self
                .validity_prover
                .get_block_merkle_proof(request.root_block_number, request.leaf_block_number)
                .await?;
            self.cache
                .set_with_ttl::<V>(&key, &block_merkle_proof, self.cache_config.static_ttl)
                .await?;
            Ok(GetBlockMerkleProofResponse { block_merkle_proof })
        }
    }

    pub async fn get_deposit_merkle_proof(
        &self,
        request: &GetDepositMerkleProofQuery,
    ) -> anyhow::Result<GetDepositMerkleProofResponse> {
        type V = DepositMerkleProof;
        let key = format!(
            "get_deposit_merkle_proof:{}",
            serde_qs::to_string(&request)?
        );
        if let Some(deposit_merkle_proof) = self.cache.get::<V>(&key).await? {
            Ok(GetDepositMerkleProofResponse {
                deposit_merkle_proof,
            })
        } else {
            let deposit_merkle_proof = self
                .validity_prover
                .get_deposit_merkle_proof(request.block_number, request.deposit_index)
                .await?;
            // the result is
            self.cache
                .set_with_ttl::<V>(&key, &deposit_merkle_proof, self.cache_config.static_ttl)
                .await?;
            Ok(GetDepositMerkleProofResponse {
                deposit_merkle_proof,
            })
        }
    }
}
