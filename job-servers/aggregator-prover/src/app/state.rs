use std::sync::Arc;

use intmax2_interfaces::utils::circuit_verifiers::CircuitVerifiers;
use intmax2_zkp::{
    circuits::claim::{
        determine_lock_time::LockTimeConfig, single_claim_processor::SingleClaimProcessor,
    },
    utils::{hash_chain::hash_chain_processor::HashChainProcessor, wrapper::WrapperCircuit},
    wrapper_config::plonky2_config::PoseidonBN128GoldilocksConfig,
};
use plonky2::{
    field::goldilocks_field::GoldilocksField,
    plonk::{circuit_data::VerifierCircuitData, config::PoseidonGoldilocksConfig},
};

type C = PoseidonGoldilocksConfig;
type OuterC = PoseidonBN128GoldilocksConfig;
const D: usize = 2;
type F = GoldilocksField;

#[derive(Clone)]
pub struct AppState {
    pub withdrawal_processor: Arc<HashChainProcessor<F, C, D>>,
    pub withdrawal_inner_wrap_circuit: Arc<WrapperCircuit<F, C, C, D>>,
    pub withdrawal_outer_wrap_circuit: Arc<WrapperCircuit<F, C, OuterC, D>>,

    pub single_claim_vd: VerifierCircuitData<F, C, D>,
    pub claim_processor: Arc<HashChainProcessor<F, C, D>>,
    pub claim_inner_wrap_circuit: Arc<WrapperCircuit<F, C, C, D>>,
    pub claim_outer_wrap_circuit: Arc<WrapperCircuit<F, C, OuterC, D>>,
}

impl AppState {
    pub fn new(is_faster_mining: bool) -> Self {
        let lock_config = if is_faster_mining {
            LockTimeConfig::faster()
        } else {
            LockTimeConfig::normal()
        };

        let single_withdrawal_vd = CircuitVerifiers::load().get_single_withdrawal_vd();
        let withdrawal_processor = Arc::new(HashChainProcessor::new(&single_withdrawal_vd));
        let withdrawal_inner_wrap_circuit = Arc::new(WrapperCircuit::new(
            &withdrawal_processor.chain_end_circuit.data.verifier_data(),
        ));
        let withdrawal_outer_wrap_circuit = Arc::new(WrapperCircuit::new(
            &withdrawal_inner_wrap_circuit.data.verifier_data(),
        ));

        let validity_vd = CircuitVerifiers::load().get_validity_vd();
        let single_claim_vd =
            SingleClaimProcessor::new(&validity_vd, &lock_config).get_verifier_data();
        let claim_processor = Arc::new(HashChainProcessor::new(&single_claim_vd));
        let claim_inner_wrap_circuit = Arc::new(WrapperCircuit::new(
            &claim_processor.chain_end_circuit.data.verifier_data(),
        ));
        let claim_outer_wrap_circuit = Arc::new(WrapperCircuit::new(
            &claim_inner_wrap_circuit.data.verifier_data(),
        ));

        Self {
            withdrawal_processor,
            withdrawal_inner_wrap_circuit,
            withdrawal_outer_wrap_circuit,
            single_claim_vd,
            claim_processor,
            claim_inner_wrap_circuit,
            claim_outer_wrap_circuit,
        }
    }
}
