use intmax2_zkp::{
    circuits::{
        balance::balance_processor::BalanceProcessor,
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

fn main() {
    let validity_processor: ValidityProcessor<F, C, D> = ValidityProcessor::new();
    let balance_processor =
        BalanceProcessor::new(&validity_processor.validity_circuit.data.verifier_data());
    let withdrawal_processor: WithdrawalProcessor<F, C, D> =
        WithdrawalProcessor::new(&balance_processor.balance_circuit.data.common);
    log::info!("The balance validity circuit build has been completed.");

    let wrapper_circuit1 = WrapperCircuit::<F, C, C, D>::new(
        &withdrawal_processor
            .withdrawal_wrapper_circuit
            .data
            .verifier_data(),
        None,
    );
    let wrapper_circuit2 =
        WrapperCircuit::<F, C, OuterC, D>::new(&wrapper_circuit1.data.verifier_data(), None);

    let common_data_path = "../gnark-server/data/withdrawal_circuit_data/common_circuit_data.json";
    let common_data = serde_json::to_string(&wrapper_circuit2.data.common).unwrap();
    std::fs::write(common_data_path, common_data).unwrap();

    let verifier_data_path =
        "../gnark-server/data/withdrawal_circuit_data/verifier_only_circuit_data.json";
    let verifier_data = serde_json::to_string(&wrapper_circuit2.data.verifier_only).unwrap();
    std::fs::write(verifier_data_path, verifier_data).unwrap();
}
