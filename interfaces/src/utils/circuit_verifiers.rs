use std::path::{Path, PathBuf};

use intmax2_zkp::circuits::{
    balance::balance_processor::BalanceProcessor,
    claim::single_claim_processor::SingleClaimProcessor,
    validity::validity_processor::ValidityProcessor,
    withdrawal::single_withdrawal_circuit::SingleWithdrawalCircuit,
};
use plonky2::{
    field::goldilocks_field::GoldilocksField,
    plonk::{circuit_data::VerifierCircuitData, config::PoseidonGoldilocksConfig},
};

use super::serializer::U32GateSerializer;

const VALIDITY_VD_BYTES: &[u8] =
    include_bytes!("../../circuit_data/validity_verifier_circuit_data.bin");
const TRANSITION_VD_BYTES: &[u8] =
    include_bytes!("../../circuit_data/transition_verifier_circuit_data.bin");
const BALANCE_VD_BYTES: &[u8] =
    include_bytes!("../../circuit_data/balance_verifier_circuit_data.bin");
const SINGLE_WITHDRAWAL_VD_BYTES: &[u8] =
    include_bytes!("../../circuit_data/single_withdrawal_verifier_circuit_data.bin");
const SINGLE_CLAIM_VD_BYTES: &[u8] =
    include_bytes!("../../circuit_data/single_claim_verifier_circuit_data.bin");

fn circuit_data_path() -> PathBuf {
    PathBuf::from("circuit_data")
}

fn balance_circuit_data_path() -> PathBuf {
    circuit_data_path().join("balance_verifier_circuit_data.bin")
}

fn validity_circuit_data_path() -> PathBuf {
    circuit_data_path().join("validity_verifier_circuit_data.bin")
}

fn transition_circuit_data_path() -> PathBuf {
    circuit_data_path().join("transition_verifier_circuit_data.bin")
}

fn single_withdrawal_circuit_data_path() -> PathBuf {
    circuit_data_path().join("single_withdrawal_verifier_circuit_data.bin")
}

fn single_claim_circuit_data_path() -> PathBuf {
    circuit_data_path().join("single_claim_verifier_circuit_data.bin")
}

type F = GoldilocksField;
type C = PoseidonGoldilocksConfig;
const D: usize = 2;

pub struct CircuitVerifiers {
    balance_vd: VerifierCircuitData<F, C, D>,
    validity_vd: VerifierCircuitData<F, C, D>,
    transition_vd: VerifierCircuitData<F, C, D>,
    single_withdrawal_vd: VerifierCircuitData<F, C, D>,
    single_claim_vd: VerifierCircuitData<F, C, D>,
}

impl CircuitVerifiers {
    // Construct the circuit verifiers from the processors.
    pub fn construct() -> Self {
        let validity_processor = ValidityProcessor::new();
        let balance_processor = BalanceProcessor::new(&validity_processor.get_verifier_data());
        let transition_vd = validity_processor
            .transition_processor
            .transition_wrapper_circuit
            .data
            .verifier_data();
        let balance_vd = balance_processor.get_verifier_data();
        let balance_common_data = balance_vd.common.clone();
        let single_withdrawal_circuit = SingleWithdrawalCircuit::new(&balance_common_data);
        let single_claim_processor =
            SingleClaimProcessor::new(&validity_processor.validity_circuit.data.verifier_data());
        Self {
            balance_vd: balance_processor.get_verifier_data(),
            validity_vd: validity_processor.validity_circuit.data.verifier_data(),
            transition_vd,
            single_withdrawal_vd: single_withdrawal_circuit.data.verifier_data(),
            single_claim_vd: single_claim_processor
                .single_claim_circuit
                .data
                .verifier_data(),
        }
    }

    pub fn save(&self) -> anyhow::Result<()> {
        save_verifier_circuit_data(&balance_circuit_data_path(), &self.balance_vd)?;
        save_verifier_circuit_data(&validity_circuit_data_path(), &self.validity_vd)?;
        save_verifier_circuit_data(&transition_circuit_data_path(), &self.transition_vd)?;
        save_verifier_circuit_data(
            &single_withdrawal_circuit_data_path(),
            &self.single_withdrawal_vd,
        )?;
        save_verifier_circuit_data(&single_claim_circuit_data_path(), &self.single_claim_vd)?;
        Ok(())
    }

    pub fn load() -> Self {
        let balance_vd = deserialize_verifier_circuit_data(BALANCE_VD_BYTES.to_vec()).unwrap();
        let validity_vd = deserialize_verifier_circuit_data(VALIDITY_VD_BYTES.to_vec()).unwrap();
        let single_withdrawal_vd =
            deserialize_verifier_circuit_data(SINGLE_WITHDRAWAL_VD_BYTES.to_vec()).unwrap();
        let transition_vd =
            deserialize_verifier_circuit_data(TRANSITION_VD_BYTES.to_vec()).unwrap();
        let single_claim_vd =
            deserialize_verifier_circuit_data(SINGLE_CLAIM_VD_BYTES.to_vec()).unwrap();
        Self {
            balance_vd,
            validity_vd,
            transition_vd,
            single_withdrawal_vd,
            single_claim_vd,
        }
    }

    pub fn get_balance_vd(&self) -> VerifierCircuitData<F, C, D> {
        self.balance_vd.clone()
    }

    pub fn get_validity_vd(&self) -> VerifierCircuitData<F, C, D> {
        self.validity_vd.clone()
    }

    pub fn get_transition_vd(&self) -> VerifierCircuitData<F, C, D> {
        self.transition_vd.clone()
    }

    pub fn get_single_withdrawal_vd(&self) -> VerifierCircuitData<F, C, D> {
        self.single_withdrawal_vd.clone()
    }

    pub fn get_single_claim_vd(&self) -> VerifierCircuitData<F, C, D> {
        self.single_claim_vd.clone()
    }
}

fn save_verifier_circuit_data(
    path: &Path,
    vd: &VerifierCircuitData<F, C, D>,
) -> anyhow::Result<()> {
    let gate_serializer = U32GateSerializer;
    let circuit_bytes = vd
        .to_bytes(&gate_serializer)
        .map_err(|e| anyhow::anyhow!(e))?;
    let mut circuit_file = std::fs::File::create(path)?;
    std::io::Write::write_all(&mut circuit_file, &circuit_bytes)?;
    Ok(())
}

fn deserialize_verifier_circuit_data(
    data: Vec<u8>,
) -> anyhow::Result<VerifierCircuitData<F, C, D>> {
    let gate_serializer = U32GateSerializer;
    let vd =
        VerifierCircuitData::from_bytes(data, &gate_serializer).map_err(|e| anyhow::anyhow!(e))?;
    Ok(vd)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use intmax2_zkp::circuits::claim::single_claim_processor::SingleClaimProcessor;

    use super::CircuitVerifiers;

    use intmax2_zkp::{
        circuits::{
            test_utils::state_manager::ValidityStateManager,
            validity::validity_processor::ValidityProcessor,
        },
        common::{
            deposit::{get_pubkey_salt_hash, Deposit},
            salt::Salt,
            signature::key_set::KeySet,
            witness::{claim_witness::ClaimWitness, deposit_time_witness::DepositTimeWitness},
        },
        ethereum_types::{address::Address, u256::U256, u32limb_trait::U32LimbTrait},
    };
    use plonky2::{
        field::goldilocks_field::GoldilocksField,
        plonk::{config::PoseidonGoldilocksConfig, proof::ProofWithPublicInputs},
    };
    use rand::Rng as _;

    #[test]
    #[ignore]
    fn test_save_circuit_verifiers() {
        let circuit_verifiers = super::CircuitVerifiers::construct();
        circuit_verifiers.save().unwrap();
    }

    #[test]
    fn test_load_circuit_verifiers() {
        let time = std::time::Instant::now();
        let _circuit_verifiers = super::CircuitVerifiers::load();
        println!("Time taken: {:?}", time.elapsed());
    }

    #[test]
    fn test_save_single_claim_verifier() {
        let circuit_verifiers = super::CircuitVerifiers::construct();
        let claim_processor = SingleClaimProcessor::new(&circuit_verifiers.get_balance_vd());
        let single_claim_vd = claim_processor.single_claim_circuit.data.verifier_data();
        println!("single_claim_vd: {:?}", single_claim_vd);
        let path = super::single_claim_circuit_data_path();
        super::save_verifier_circuit_data(&path, &single_claim_vd).unwrap();
    }

    type F = GoldilocksField;
    type C = PoseidonGoldilocksConfig;
    const D: usize = 2;

    const LOCK_TIME_MAX: u32 = 432000; // 5 days

    fn generate_claim_and_validity_proof() -> (
        ProofWithPublicInputs<F, C, D>,
        ProofWithPublicInputs<F, C, D>,
    ) {
        let mut rng = rand::thread_rng();
        let validity_processor = Arc::new(ValidityProcessor::<F, C, D>::new());
        let mut validity_state_manager = ValidityStateManager::new(validity_processor.clone());
        let single_claim_processor =
            SingleClaimProcessor::new(&validity_processor.get_verifier_data());

        let key = KeySet::rand(&mut rng);

        // deposit
        let deposit_salt = Salt::rand(&mut rng);
        let deposit_salt_hash = get_pubkey_salt_hash(key.pubkey, deposit_salt);
        let deposit = Deposit {
            depositor: Address::rand(&mut rng),
            pubkey_salt_hash: deposit_salt_hash,
            amount: U256::rand_small(&mut rng),
            token_index: rng.gen(),
            is_eligible: true,
        };
        let deposit_index = validity_state_manager.deposit(&deposit).unwrap();

        // post empty block to sync deposit tree
        validity_state_manager.tick(false, &[], 0).unwrap();
        let validity_proof = validity_state_manager.validity_proof.clone().unwrap();

        // lock time max passed in this block
        validity_state_manager
            .tick(false, &[], LOCK_TIME_MAX as u64)
            .unwrap();

        let update_witness = validity_state_manager
            .get_update_witness(key.pubkey, 2, 1, false)
            .unwrap();
        let deposit_time_public_witness = validity_state_manager
            .get_deposit_time_public_witness(1, deposit_index)
            .unwrap();

        let deposit_time_witness = DepositTimeWitness {
            public_witness: deposit_time_public_witness,
            deposit_index,
            deposit,
            deposit_salt,
            pubkey: key.pubkey,
        };
        let recipient = Address::rand(&mut rng);
        let claim_witness = ClaimWitness {
            recipient,
            deposit_time_witness,
            update_witness,
        };

        let single_claim_proof = single_claim_processor.prove(&claim_witness).unwrap();
        single_claim_processor
            .single_claim_circuit
            .data
            .verify(single_claim_proof.clone())
            .expect("Verification failed for single claim");

        (single_claim_proof, validity_proof)
    }

    #[test]
    fn test_claim() {
        let (single_claim_proof, validity_proof) = generate_claim_and_validity_proof();
        let circuit_verifiers = CircuitVerifiers::load();
        let single_claim_vd = circuit_verifiers.get_single_claim_vd();
        single_claim_vd
            .verify(single_claim_proof)
            .expect("Verification failed for single claim using verifier data");

        let validity_vd = circuit_verifiers.get_validity_vd();
        validity_vd
            .verify(validity_proof)
            .expect("Verification failed for validity using verifier data");
    }
}
