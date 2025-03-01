use crate::{app::validity_prover::ValidityProver, Env};

#[derive(Clone)]
pub struct State {
    pub validity_prover: ValidityProver,
}

impl State {
    pub async fn new(env: &Env) -> anyhow::Result<Self> {
        let validity_prover = ValidityProver::new(env).await?;
        Ok(Self { validity_prover })
    }

    pub async fn job(&self) {
        self.clone().validity_prover.job().await.unwrap();
    }
}
