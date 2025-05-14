use intmax2_client_sdk::external_api::contract::{
    liquidity_contract::LiquidityContract,
    rollup_contract::{FullBlockWithMeta, RollupContract},
};
use intmax2_zkp::common::witness::full_block::FullBlock;
use server_common::db::{DbPool, DbPoolConfig};

use crate::EnvVar;

use super::{
    error::ObserverError, leader_election::LeaderElection, observer_api::ObserverApi,
    observer_rpc::ObserverConfig,
};

#[derive(Clone)]
pub struct TheGraphObserver {
    pub config: ObserverConfig,
    pub rollup_contract: RollupContract,
    pub liquidity_contract: LiquidityContract,
    pub observer_api: ObserverApi,
    pub leader_election: LeaderElection,
    pub pool: DbPool,
}

impl TheGraphObserver {
    pub async fn new(env: &EnvVar, observer_api: ObserverApi) -> Result<Self, ObserverError> {
        let config = ObserverConfig {
            observer_event_block_interval: env.observer_event_block_interval,
            observer_max_query_times: env.observer_max_query_times,
            observer_sync_interval: env.observer_sync_interval,
            observer_restart_interval: env.observer_restart_interval,
            rollup_contract_deployed_block_number: env.rollup_contract_deployed_block_number,
            liquidity_contract_deployed_block_number: env.liquidity_contract_deployed_block_number,
        };
        tracing::info!("Observer config: {:?}", config);
        let pool = DbPool::from_config(&DbPoolConfig {
            max_connections: env.database_max_connections,
            idle_timeout: env.database_timeout,
            url: env.database_url.to_string(),
        })
        .await?;
        let leader_election = LeaderElection::new(
            &env.redis_url,
            "validity_prover:sync_leader",
            std::time::Duration::from_secs(env.leader_lock_ttl),
        )?;

        // Initialize with genesis block if table is empty
        let count = sqlx::query!("SELECT COUNT(*) as count FROM full_blocks")
            .fetch_one(&pool)
            .await?
            .count
            .unwrap_or(0);
        if count == 0 {
            let genesis = FullBlockWithMeta {
                full_block: FullBlock::genesis(),
                eth_block_number: 0,
                eth_tx_index: 0,
            };
            // Insert genesis block
            sqlx::query!(
                "INSERT INTO full_blocks (block_number, eth_block_number, eth_tx_index, full_block) 
                 VALUES ($1, $2, $3, $4)",
                0i32,
                genesis.eth_block_number as i64,
                genesis.eth_tx_index as i64,
                bincode::serialize(&genesis.full_block).unwrap()
            )
            .execute(&pool)
            .await?;
        }
        Ok(Self {
            config,
            rollup_contract: observer_api.rollup_contract.clone(),
            liquidity_contract: observer_api.liquidity_contract.clone(),
            observer_api,
            leader_election,
            pool,
        })
    }
}
