use std::sync::{Arc, OnceLock};

use intmax2_zkp::{
    circuits::{
        balance::{balance_circuit::BalanceCircuit, balance_processor::BalanceProcessor},
        validity::validity_processor::ValidityProcessor,
        withdrawal::withdrawal_processor::WithdrawalProcessor,
    },
    utils::wrapper::WrapperCircuit,
    wrapper_config::plonky2_config::PoseidonBN128GoldilocksConfig,
};
use plonky2::{field::goldilocks_field::GoldilocksField, plonk::config::PoseidonGoldilocksConfig};

type C = PoseidonGoldilocksConfig;
type OuterC = PoseidonBN128GoldilocksConfig;
const D: usize = 2;
type F = GoldilocksField;

pub struct AppState {
    pub withdrawal_processor: Arc<OnceLock<WithdrawalProcessor<F, C, D>>>,
    pub balance_circuit: Arc<OnceLock<BalanceCircuit<F, C, D>>>,
}

impl AppState {
    pub fn new() -> Self {
        let withdrawal_processor = Arc::new(OnceLock::new());
        let balance_circuit = Arc::new(OnceLock::new());
        let wrapper_circuit1 = Arc::new(OnceLock::new());
        let wrapper_circuit2 = Arc::new(OnceLock::new());
        let _: tokio::task::JoinHandle<()> = tokio::spawn(build_circuits(
            Arc::clone(&withdrawal_processor),
            Arc::clone(&balance_circuit),
            Arc::clone(&wrapper_circuit1),
            Arc::clone(&wrapper_circuit2),
        ));

        Self {
            withdrawal_processor,
            balance_circuit,
        }
    }
}

impl Clone for AppState {
    fn clone(&self) -> Self {
        Self {
            withdrawal_processor: Arc::clone(&self.withdrawal_processor),
            balance_circuit: Arc::clone(&self.balance_circuit),
        }
    }
}

async fn build_circuits(
    withdrawal_processor_state: Arc<OnceLock<WithdrawalProcessor<F, C, D>>>,
    balance_circuit_state: Arc<OnceLock<BalanceCircuit<F, C, D>>>,
    wrapper_circuit1_state: Arc<OnceLock<WrapperCircuit<F, C, C, D>>>,
    wrapper_circuit2_state: Arc<OnceLock<WrapperCircuit<F, C, OuterC, D>>>,
) {
    let validity_processor = ValidityProcessor::new();
    let balance_processor =
        BalanceProcessor::new(&validity_processor.validity_circuit.data.verifier_data());
    let withdrawal_processor =
        WithdrawalProcessor::new(&balance_processor.balance_circuit.data.common);
    let wrapper_circuit1 = WrapperCircuit::<F, C, C, D>::new(
        &withdrawal_processor
            .withdrawal_wrapper_circuit
            .data
            .verifier_data(),
        None,
    );
    let wrapper_circuit2 =
        WrapperCircuit::<F, C, OuterC, D>::new(&wrapper_circuit1.data.verifier_data(), None);
    log::info!("The balance validity circuit build has been completed.");

    let _ = withdrawal_processor_state.get_or_init(|| withdrawal_processor);
    let _ = balance_circuit_state.get_or_init(|| balance_processor.balance_circuit);
    let _ = wrapper_circuit1_state.get_or_init(|| wrapper_circuit1);
    let _ = wrapper_circuit2_state.get_or_init(|| wrapper_circuit2);
}
