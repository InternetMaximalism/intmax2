use std::sync::Arc;

use intmax2_interfaces::utils::circuit_verifiers::CircuitVerifiers;
use intmax2_zkp::{
    utils::{hash_chain::hash_chain_processor::HashChainProcessor, wrapper::WrapperCircuit},
    wrapper_config::plonky2_config::PoseidonBN128GoldilocksConfig,
};
use plonky2::{field::goldilocks_field::GoldilocksField, plonk::config::PoseidonGoldilocksConfig};

type C = PoseidonGoldilocksConfig;
type OuterC = PoseidonBN128GoldilocksConfig;
const D: usize = 2;
type F = GoldilocksField;

#[derive(Clone)]
pub struct AppState {
    pub withdrawal_processor: Arc<HashChainProcessor<F, C, D>>,
    pub inner_wrap_circuit: Arc<WrapperCircuit<F, C, C, D>>,
    pub outer_wrap_circuit: Arc<WrapperCircuit<F, C, OuterC, D>>,
}

impl Default for AppState {
    fn default() -> Self {
        let single_withdrawal_vd = CircuitVerifiers::load().get_single_withdrawal_vd();
        let withdrawal_processor = Arc::new(HashChainProcessor::new(&single_withdrawal_vd));
        let inner_wrap_circuit = Arc::new(WrapperCircuit::new(
            &withdrawal_processor.chain_end_circuit.data.verifier_data(),
        ));
        let outer_wrap_circuit = Arc::new(WrapperCircuit::new(
            &inner_wrap_circuit.data.verifier_data(),
        ));
        Self {
            withdrawal_processor,
            inner_wrap_circuit,
            outer_wrap_circuit,
        }
    }
}
