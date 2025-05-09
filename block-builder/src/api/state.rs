use intmax2_client_sdk::external_api::contract::utils::get_provider_with_fallback;

use crate::{
    app::{block_builder::BlockBuilder, error::BlockBuilderError},
    EnvVar,
};

#[derive(Clone)]
pub struct State {
    pub block_builder: BlockBuilder,
}

impl State {
    pub async fn new(env: &EnvVar) -> Result<Self, BlockBuilderError> {
        let provider = get_provider_with_fallback(&[env.l2_rpc_url.clone()])?;
        let block_builder = BlockBuilder::new(env, provider).await?;
        Ok(State { block_builder })
    }

    pub fn run(&self) {
        self.block_builder.run();
    }
}

#[cfg(test)]
mod tests {
    use std::panic::AssertUnwindSafe;

    use alloy::primitives::Address;

    use super::*;

    use crate::app::storage::redis_storage::test_redis_helper::{
        assert_and_stop, find_free_port, run_redis_docker, stop_redis_docker,
    };

    // Tries to create new State using locally initialized EnvVar
    #[tokio::test]
    async fn test_state_new_redis_storage() {
        let port = find_free_port();
        let cont_name = "redis-test-state-new-redis-storage";

        // Initialize our own EnvVar
        let env = EnvVar {
            port: 9004,
            block_builder_url: "http://localhost:9004".to_string(),
            redis_url: Some(format!("redis://localhost:{}", port).to_string()),
            cluster_id: Some("1".to_string()),
            l2_rpc_url: "http://localhost:8545".to_string(),
            rollup_contract_address: Address::default(),
            block_builder_registry_contract_address: Address::default(),
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

        // Create new State
        let state = State::new(&env).await;
        assert_and_stop(
            cont_name,
            AssertUnwindSafe(|| {
                assert!(
                    state.is_ok(),
                    "Couldn't create new State using locally initialized EnvVar"
                )
            }),
        );

        // Stop docker image
        let output = stop_redis_docker(cont_name);
        assert!(
            output.status.success(),
            "Couldn't stop {}: {}",
            cont_name,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // Tries to create new State using locally initialized EnvVar
    #[tokio::test]
    async fn test_state_new_memory_storage() {
        // Initialize our own EnvVar
        let env = EnvVar {
            port: 9004,
            block_builder_url: "http://localhost:9004".to_string(),
            redis_url: None,
            cluster_id: Some("1".to_string()),
            l2_rpc_url: "http://localhost:8545".to_string(),
            rollup_contract_address: Address::default(),
            block_builder_registry_contract_address: Address::default(),
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
            beneficiary_pubkey: None,
            registration_fee: Some("0:100,1:2000".to_string()),
            non_registration_fee: Some("0:100,1:2000".to_string()),
            registration_collateral_fee: None,
            non_registration_collateral_fee: None,
        };

        // Create new State
        let state = State::new(&env).await;
        assert!(
            state.is_ok(),
            "Couldn't create new State with an empty Redis url"
        );
    }
}
