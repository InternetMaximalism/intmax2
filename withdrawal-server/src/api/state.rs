use std::sync::Arc;

use crate::{app::withdrawal_server::WithdrawalServer, Env};

#[derive(Clone)]
pub struct State {
    pub withdrawal_server: Arc<WithdrawalServer>,
}

impl State {
    pub async fn new(env: &Env) -> anyhow::Result<Self> {
        let withdrawal_server = WithdrawalServer::new(env).await?;
        Ok(State {
            withdrawal_server: Arc::new(withdrawal_server),
        })
    }
}
