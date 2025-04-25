use std::sync::Arc;

use super::validity_prover::ValidityProver;

#[derive(Clone)]
pub struct State {
    pub validity_prover: Arc<ValidityProver>,
}

impl State {
    pub fn new(validity_prover: ValidityProver) -> Self {
        let _ = validity_prover.validity_processor(); // initialize
        log::info!("State initialized");
        Self {
            validity_prover: Arc::new(validity_prover),
        }
    }
}
