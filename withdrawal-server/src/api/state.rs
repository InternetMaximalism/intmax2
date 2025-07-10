use std::sync::Arc;

use intmax2_client_sdk::external_api::contract::utils::get_provider_with_fallback;
use server_common::parser::parse_urls;

use crate::{app::withdrawal_server::WithdrawalServer, Env};

#[derive(Clone)]
pub struct State {
    pub withdrawal_server: Arc<WithdrawalServer>,
}

impl State {
    pub async fn new(env: &Env) -> anyhow::Result<Self> {
        let l2_rpc_urls = parse_urls(&env.l2_rpc_url)?;
        let provider = get_provider_with_fallback(l2_rpc_urls.as_ref())?;
        let withdrawal_server = WithdrawalServer::new(env, provider).await?;
        Ok(State {
            withdrawal_server: Arc::new(withdrawal_server),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dotenvy::dotenv;

    #[tokio::test]
    async fn test_state_creation_from_env() {
        dotenv().ok();
        let env = envy::from_env().expect("Failed to parse env");

        let state = State::new(&env).await.expect("State creation failed");

        assert!(Arc::strong_count(&state.withdrawal_server) >= 1);
    }
}
