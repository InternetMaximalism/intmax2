use intmax2_interfaces::utils::circuit_verifiers::CircuitVerifiers;
use intmax2_zkp::{
    circuits::{
        claim::{
            determine_lock_time::LockTimeConfig, single_claim_processor::SingleClaimProcessor,
        },
        withdrawal::single_withdrawal_circuit::SingleWithdrawalCircuit,
    },
    common::witness::{
        claim_witness::ClaimWitness, receive_deposit_witness::ReceiveDepositWitness,
        receive_transfer_witness::ReceiveTransferWitness, spent_witness::SpentWitness,
        tx_witness::TxWitness, update_witness::UpdateWitness,
        withdrawal_witness::WithdrawalWitness,
    },
    ethereum_types::u256::U256,
};
use plonky2::{
    field::goldilocks_field::GoldilocksField,
    plonk::{
        circuit_data::VerifierCircuitData, config::PoseidonGoldilocksConfig,
        proof::ProofWithPublicInputs,
    },
};

use intmax2_zkp::circuits::balance::balance_processor::BalanceProcessor;

use super::error::BalanceProverError;

type F = GoldilocksField;
type C = PoseidonGoldilocksConfig;
const D: usize = 2;

pub struct BalanceProver {
    pub validity_vd: VerifierCircuitData<F, C, D>,
    pub balance_vd: VerifierCircuitData<F, C, D>,
    pub balance_processor: BalanceProcessor<F, C, D>,
    pub single_withdrawal_circuit: SingleWithdrawalCircuit<F, C, D>,
    pub single_claim_processor: SingleClaimProcessor<F, C, D>,
    pub single_faster_claim_processor: SingleClaimProcessor<F, C, D>,
}

impl BalanceProver {
    pub fn new() -> anyhow::Result<Self> {
        let verifiers = CircuitVerifiers::load();

        let validity_vd = verifiers.get_validity_vd();
        let balance_processor = BalanceProcessor::new(&validity_vd);
        let balance_vd = balance_processor
            .balance_circuit
            .data
            .verifier_data()
            .clone();
        let single_withdrawal_circuit = SingleWithdrawalCircuit::new(&balance_vd);
        let single_claim_processor =
            SingleClaimProcessor::new(&validity_vd, &LockTimeConfig::normal());

        let single_faster_claim_processor =
            SingleClaimProcessor::new(&validity_vd, &LockTimeConfig::faster());

        Ok(Self {
            validity_vd,
            balance_vd,
            balance_processor,
            single_withdrawal_circuit,
            single_claim_processor,
            single_faster_claim_processor,
        })
    }

    pub fn prove_spent(
        &self,
        spent_witness: &SpentWitness,
    ) -> Result<ProofWithPublicInputs<F, C, D>, BalanceProverError> {
        let spent_proof = self
            .balance_processor
            .balance_transition_processor
            .sender_processor
            .prove_spent(spent_witness)
            .map_err(|e| BalanceProverError::ProveSpentError(e.to_string()))?;
        Ok(spent_proof)
    }

    pub fn prove_send(
        &self,
        pubkey: U256,
        tx_witness: &TxWitness,
        update_witness: &UpdateWitness<F, C, D>,
        spent_proof: &ProofWithPublicInputs<F, C, D>,
        prev_proof: &Option<ProofWithPublicInputs<F, C, D>>,
    ) -> Result<ProofWithPublicInputs<F, C, D>, BalanceProverError> {
        let balance_proof = self
            .balance_processor
            .prove_send(
                &self.validity_vd,
                pubkey,
                tx_witness,
                update_witness,
                spent_proof,
                prev_proof,
            )
            .map_err(|e| BalanceProverError::ProveSendError(e.to_string()))?;
        Ok(balance_proof)
    }

    pub fn prove_update(
        &self,
        pubkey: U256,
        update_witness: &UpdateWitness<F, C, D>,
        prev_proof: &Option<ProofWithPublicInputs<F, C, D>>,
    ) -> Result<ProofWithPublicInputs<F, C, D>, BalanceProverError> {
        let balance_proof = self
            .balance_processor
            .prove_update(&self.validity_vd, pubkey, update_witness, prev_proof)
            .map_err(|e| BalanceProverError::ProveUpdateError(e.to_string()))?;
        Ok(balance_proof)
    }

    pub fn prove_receive_transfer(
        &self,
        pubkey: U256,
        receive_transfer_witness: &ReceiveTransferWitness<F, C, D>,
        prev_proof: &Option<ProofWithPublicInputs<F, C, D>>,
    ) -> Result<ProofWithPublicInputs<F, C, D>, BalanceProverError> {
        let balance_proof = self
            .balance_processor
            .prove_receive_transfer(pubkey, receive_transfer_witness, prev_proof)
            .map_err(|e| BalanceProverError::ProveReceiveTransferError(e.to_string()))?;
        Ok(balance_proof)
    }

    pub fn prove_receive_deposit(
        &self,
        pubkey: U256,
        receive_deposit_witness: &ReceiveDepositWitness,
        prev_proof: &Option<ProofWithPublicInputs<F, C, D>>,
    ) -> Result<ProofWithPublicInputs<F, C, D>, BalanceProverError> {
        let balance_proof = self
            .balance_processor
            .prove_receive_deposit(pubkey, receive_deposit_witness, prev_proof)
            .map_err(|e| BalanceProverError::ProveReceiveDepositError(e.to_string()))?;
        Ok(balance_proof)
    }

    pub fn prove_single_withdrawal(
        &self,
        withdrawal_witness: &WithdrawalWitness<F, C, D>,
    ) -> Result<ProofWithPublicInputs<F, C, D>, BalanceProverError> {
        let transition_inclusion_value = withdrawal_witness
            .to_transition_inclusion_value(&self.balance_vd)
            .map_err(|e| BalanceProverError::ProveSingleWithdrawalError(e.to_string()))?;
        let single_withdrawal_proof = self
            .single_withdrawal_circuit
            .prove(&transition_inclusion_value)
            .map_err(|e| BalanceProverError::ProveSingleWithdrawalError(e.to_string()))?;
        Ok(single_withdrawal_proof)
    }

    pub fn prove_single_claim(
        &self,
        is_faster_mining: bool,
        claim_witness: &ClaimWitness<F, C, D>,
    ) -> Result<ProofWithPublicInputs<F, C, D>, BalanceProverError> {
        let single_claim_processor = if is_faster_mining {
            &self.single_faster_claim_processor
        } else {
            &self.single_claim_processor
        };
        let single_claim_proof = single_claim_processor
            .prove(claim_witness)
            .map_err(|e| BalanceProverError::ProveSingleWithdrawalError(e.to_string()))?;
        Ok(single_claim_proof)
    }
}
