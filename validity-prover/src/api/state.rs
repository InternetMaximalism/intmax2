use std::time::Duration;

use intmax2_interfaces::api::validity_prover::types::{
    GetAccountInfoQuery, GetAccountInfoResponse,
};
use server_common::redis::cache::RedisCache;

use crate::{app::validity_prover::ValidityProver, Env};

const DYNAMIC_TTL: Duration = Duration::from_secs(5);
const STATIC_TTL: Duration = Duration::from_secs(3600); // 1 hour

pub struct State {
    pub validity_prover: ValidityProver,
    pub cache: RedisCache,
}

impl State {
    pub async fn new(env: &Env) -> anyhow::Result<Self> {
        let validity_prover = ValidityProver::new(env).await?;
        let cache = RedisCache::new(&env.redis_url, "validity_prover:cache")?;
        Ok(Self {
            validity_prover,
            cache,
        })
    }

    pub async fn job(&self) {
        self.validity_prover.clone().job().await.unwrap();
    }

    pub async fn get_block_number(&self) -> anyhow::Result<u32> {
        let key = "block_number";
        if let Some(block_number) = self.cache.get(key).await? {
            Ok(block_number)
        } else {
            let block_number = self.validity_prover.get_last_block_number().await?;
            self.cache
                .set_with_ttl(key, &block_number, DYNAMIC_TTL)
                .await?;
            Ok(block_number)
        }
    }

    pub async fn get_validity_proof_block_number(&self) -> anyhow::Result<u32> {
        let key = "validity_proof_block_number";
        if let Some(block_number) = self.cache.get(key).await? {
            Ok(block_number)
        } else {
            let block_number = self
                .validity_prover
                .get_latest_validity_proof_block_number()
                .await?;
            self.cache
                .set_with_ttl(key, &block_number, DYNAMIC_TTL)
                .await?;
            Ok(block_number)
        }
    }

    pub async fn get_next_deposit_index(&self) -> anyhow::Result<u32> {
        let key = "next_deposit_index";
        if let Some(deposit_index) = self.cache.get(key).await? {
            Ok(deposit_index)
        } else {
            let deposit_index = self.validity_prover.get_next_deposit_index().await?;
            self.cache
                .set_with_ttl(key, &deposit_index, DYNAMIC_TTL)
                .await?;
            Ok(deposit_index)
        }
    }

    pub async fn get_latest_included_deposit_index(&self) -> anyhow::Result<Option<u32>> {
        let key = "latest_included_deposit_index";
        if let Some(deposit_index) = self.cache.get(key).await? {
            Ok(deposit_index)
        } else {
            let deposit_index = self
                .validity_prover
                .get_latest_included_deposit_index()
                .await?;
            self.cache
                .set_with_ttl(key, &deposit_index, DYNAMIC_TTL)
                .await?;
            Ok(deposit_index)
        }
    }

    pub async fn get_account_info(
        &self,
        request: GetAccountInfoQuery,
    ) -> anyhow::Result<GetAccountInfoResponse> {
        let key = format!("get_account_info:{}", serde_qs::to_string(&request)?);
        if let Some(account_info) = self.cache.get(&key).await? {
            Ok(account_info)
        } else {
            let account_info = self
                .validity_prover
                .get_account_info(request.pubkey)
                .await?;
            self.cache
                .set_with_ttl(&key, &account_info, STATIC_TTL)
                .await?;
            Ok(GetAccountInfoResponse { account_info })
        }
    }
}
