use std::sync::Arc;

use intmax2_client_sdk::external_api::contract::utils::get_provider_with_fallback;

use crate::{app::withdrawal_server::WithdrawalServer, Env};

#[derive(Clone)]
pub struct State {
    pub withdrawal_server: Arc<WithdrawalServer>,
}

impl State {
    pub async fn new(env: &Env) -> anyhow::Result<Self> {
        let provider = get_provider_with_fallback(&[env.l2_rpc_url.clone()])?;
        let withdrawal_server = WithdrawalServer::new(env, provider).await?;
        Ok(State {
            withdrawal_server: Arc::new(withdrawal_server),
        })
    }
}
