use intmax2_zkp::{
    common::witness::{
        receive_deposit_witness::ReceiveDepositWitness,
        receive_transfer_witness::ReceiveTransferWitness, spent_witness::SpentWitness,
        tx_witness::TxWitness, update_witness::UpdateWitness,
        withdrawal_witness::WithdrawalWitness,
    },
    ethereum_types::u256::U256,
};
use plonky2::{
    field::goldilocks_field::GoldilocksField,
    plonk::{config::PoseidonGoldilocksConfig, proof::ProofWithPublicInputs},
};

type F = GoldilocksField;
type C = PoseidonGoldilocksConfig;
const D: usize = 2;

pub struct ProveResponse {
    pub proof: ProofWithPublicInputs<F, C, D>,
}

pub struct ProveSpentRequest {
    pub spent_witness: SpentWitness,
}

pub struct ProveSendRequest {
    pub pubkey: U256,
    pub tx_witnes: TxWitness,
    pub update_witness: UpdateWitness<F, C, D>,
    pub spent_proof: ProofWithPublicInputs<F, C, D>,
    pub prev_proof: Option<ProofWithPublicInputs<F, C, D>>,
}

pub struct ProveUpdateRequest {
    pub pubkey: U256,
    pub update_witness: UpdateWitness<F, C, D>,
    pub prev_proof: Option<ProofWithPublicInputs<F, C, D>>,
}

pub struct ProveReceiveTransferRequest {
    pub pubkey: U256,
    pub receive_transfer_witness: ReceiveTransferWitness<F, C, D>,
    pub prev_proof: Option<ProofWithPublicInputs<F, C, D>>,
}

pub struct ProveReceiveDepositRequest {
    pub pubkey: U256,
    pub receive_deposit_witness: ReceiveDepositWitness,
    pub prev_proof: Option<ProofWithPublicInputs<F, C, D>>,
}

pub struct ProveSingleWithdrawalRequest {
    pub withdrawal_witness: WithdrawalWitness<F, C, D>,
}
