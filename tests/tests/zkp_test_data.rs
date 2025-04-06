use intmax2_client_sdk::client::sync::utils::generate_spent_witness;
use intmax2_interfaces::api::{
    balance_prover::types::{
        ProveReceiveDepositRequest, ProveReceiveTransferRequest, ProveSendRequest,
        ProveSingleClaimRequest, ProveSingleWithdrawalRequest, ProveSpentRequest,
        ProveUpdateRequest,
    },
    private_zkp_server::types::{ProveRequestWithType, ProveType},
};
use plonky2::{field::goldilocks_field::GoldilocksField, plonk::config::PoseidonGoldilocksConfig};
use std::{fs, path::PathBuf, str::FromStr, sync::Arc};

use intmax2_zkp::{
    circuits::{
        balance::balance_processor::BalanceProcessor,
        claim::determine_lock_time::LockTimeConfig,
        test_utils::{state_manager::ValidityStateManager, witness_generator::MockTxRequest},
        validity::validity_processor::ValidityProcessor,
    },
    common::{
        deposit::{get_pubkey_salt_hash, Deposit},
        generic_address::GenericAddress,
        private_state::FullPrivateState,
        salt::Salt,
        signature::key_set::KeySet,
        transfer::Transfer,
        trees::transfer_tree::TransferTree,
        witness::{
            claim_witness::ClaimWitness, deposit_time_witness::DepositTimeWitness,
            deposit_witness::DepositWitness, private_transition_witness::PrivateTransitionWitness,
            receive_deposit_witness::ReceiveDepositWitness,
            receive_transfer_witness::ReceiveTransferWitness, transfer_witness::TransferWitness,
            withdrawal_witness::WithdrawalWitness,
        },
    },
    constants::TRANSFER_TREE_HEIGHT,
    ethereum_types::{address::Address, bytes32::Bytes32, u256::U256, u32limb_trait::U32LimbTrait},
};
use rand::Rng as _;

type F = GoldilocksField;
type C = PoseidonGoldilocksConfig;
const D: usize = 2;

#[test]
#[ignore]
fn generate_zkp_test_data() {
    let mut rng = rand::thread_rng();
    let lock_config = LockTimeConfig::normal();
    let validity_processor = Arc::new(ValidityProcessor::<F, C, D>::new());
    let balance_processor = BalanceProcessor::new(&validity_processor.get_verifier_data());
    let mut validity_state_manager =
        ValidityStateManager::new(validity_processor.clone(), Address::default());
    let key = KeySet::rand(&mut rng);
    let mut private_state = FullPrivateState::new();
    let mut balance_proof = None;

    let recipient = KeySet::rand(&mut rng);
    let mut recipient_private_state = FullPrivateState::new();
    let mut recipient_balance_proof = None;

    let spent_circuit = balance_processor.spent_circuit();

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
    validity_state_manager.tick(false, &[], 0, 0).unwrap();
    let receive_deposit_block_number = validity_state_manager.get_block_number();

    // lock time max passed in this block
    validity_state_manager
        .tick(false, &[], 0, lock_config.lock_time_max as u64)
        .unwrap();
    let mature_block_number = validity_state_manager.get_block_number();

    // generate claim witness
    {
        let update_witness = validity_state_manager
            .get_update_witness(
                key.pubkey,
                mature_block_number,
                receive_deposit_block_number,
                false,
            )
            .unwrap();
        let deposit_time_public_witness = validity_state_manager
            .get_deposit_time_public_witness(receive_deposit_block_number, deposit_index)
            .unwrap();
        let deposit_time_witness = DepositTimeWitness {
            public_witness: deposit_time_public_witness,
            deposit_index,
            deposit: deposit.clone(),
            deposit_salt,
            pubkey: key.pubkey,
        };
        let recipient = Address::rand(&mut rng);
        let claim_witness = ClaimWitness {
            recipient,
            deposit_time_witness,
            update_witness,
        };

        // faster mining
        {
            let request = ProveSingleClaimRequest {
                is_faster_mining: true,
                claim_witness: claim_witness.clone(),
            };
            let request = ProveRequestWithType {
                prove_type: ProveType::SingleClaim,
                pubkey: key.pubkey,
                request: bincode::serialize(&request).unwrap(),
            };
            save_proof_request("faster_", &request);
        }

        // normal mining
        {
            let request = ProveSingleClaimRequest {
                is_faster_mining: false,
                claim_witness: claim_witness.clone(),
            };
            let request = ProveRequestWithType {
                prove_type: ProveType::SingleClaim,
                pubkey: key.pubkey,
                request: bincode::serialize(&request).unwrap(),
            };
            save_proof_request("", &request);
        }
    }

    // update balance proof
    {
        let update_witness = validity_state_manager
            .get_update_witness(key.pubkey, mature_block_number, 0, false)
            .unwrap();

        let request = ProveUpdateRequest {
            pubkey: key.pubkey,
            update_witness: update_witness.clone(),
            prev_proof: balance_proof.clone(),
        };
        let request = ProveRequestWithType {
            prove_type: ProveType::Update,
            pubkey: key.pubkey,
            request: bincode::serialize(&request).unwrap(),
        };
        save_proof_request("", &request);

        // update balance proof
        balance_proof = Some(
            balance_processor
                .prove_update(
                    &validity_processor.get_verifier_data(),
                    key.pubkey,
                    &update_witness,
                    &balance_proof,
                )
                .unwrap(),
        );
    }

    // sync deposit
    {
        let deposit_merkle_proof = validity_state_manager
            .get_deposit_merkle_proof(mature_block_number, deposit_index)
            .unwrap();
        let deposit_witness = DepositWitness {
            deposit_salt,
            deposit_index,
            deposit: deposit.clone(),
            deposit_merkle_proof,
        };
        let nullifier: Bytes32 = deposit.poseidon_hash().into();
        let new_salt = Salt::rand(&mut rng);
        let private_transition_witness = PrivateTransitionWitness::new(
            &mut private_state,
            deposit.token_index,
            deposit.amount,
            nullifier,
            new_salt,
        )
        .unwrap();
        let receive_deposit_witness = ReceiveDepositWitness {
            deposit_witness,
            private_transition_witness,
        };
        let request = ProveReceiveDepositRequest {
            pubkey: key.pubkey,
            receive_deposit_witness: receive_deposit_witness.clone(),
            prev_proof: balance_proof.clone(),
        };
        let request = ProveRequestWithType {
            prove_type: ProveType::ReceiveDeposit,
            pubkey: key.pubkey,
            request: bincode::serialize(&request).unwrap(),
        };
        save_proof_request("", &request);

        // update balance proof
        balance_proof = Some(
            balance_processor
                .prove_receive_deposit(key.pubkey, &receive_deposit_witness, &balance_proof)
                .unwrap(),
        );
    }

    // generate transfer
    let transfer = Transfer {
        recipient: GenericAddress::from_pubkey(recipient.pubkey),
        token_index: deposit.token_index,
        amount: 1.into(),
        salt: Salt::rand(&mut rng),
    };
    let withdrawal_transfer = Transfer {
        recipient: GenericAddress::from_address(Address::default()),
        token_index: deposit.token_index,
        amount: 1.into(),
        salt: Salt::rand(&mut rng),
    };
    let transfers = vec![transfer, withdrawal_transfer];

    // generate spent witness
    let tx_nonce = private_state.nonce;
    let spent_witness = generate_spent_witness(&private_state, tx_nonce, &transfers).unwrap();

    // save spent proof request
    {
        let request = ProveSpentRequest {
            spent_witness: spent_witness.clone(),
        };
        let result = ProveRequestWithType {
            prove_type: ProveType::Spent,
            pubkey: key.pubkey,
            request: bincode::serialize(&request).unwrap(),
        };
        save_proof_request("", &result);
    }
    let spent_proof = spent_circuit
        .prove(&spent_witness.to_value().unwrap())
        .unwrap();

    // generate transfer witness
    let mut transfer_tree = TransferTree::new(TRANSFER_TREE_HEIGHT);
    for &transfer in &transfers {
        transfer_tree.push(transfer);
    }
    let transfer_witnesses = transfers
        .iter()
        .enumerate()
        .map(|(transfer_index, &transfer)| {
            let transfer_merkle_proof = transfer_tree.prove(transfer_index as u64);
            TransferWitness {
                tx: spent_witness.tx,
                transfer,
                transfer_index: transfer_index as u32,
                transfer_merkle_proof,
            }
        })
        .collect::<Vec<_>>();

    // post block
    let transfer_request = MockTxRequest {
        tx: spent_witness.tx,
        sender_key: key,
        will_return_sig: true,
    };
    let tx_witnesses = validity_state_manager
        .tick(
            true,
            &[transfer_request],
            0,
            lock_config.lock_time_max as u64 + 1,
        )
        .unwrap();
    let tx_witness = tx_witnesses[0].clone();
    let sent_block_number = validity_state_manager.get_block_number();

    // sync send
    {
        let update_witness = validity_state_manager
            .get_update_witness(key.pubkey, sent_block_number, mature_block_number, true)
            .unwrap();
        // update private state
        spent_witness
            .update_private_state(&mut private_state)
            .unwrap();

        let request = ProveSendRequest {
            pubkey: key.pubkey,
            tx_witness: tx_witness.clone(),
            update_witness: update_witness.clone(),
            spent_proof: spent_proof.clone(),
            prev_proof: balance_proof.clone(),
        };
        let result = ProveRequestWithType {
            prove_type: ProveType::Send,
            pubkey: key.pubkey,
            request: bincode::serialize(&request).unwrap(),
        };
        save_proof_request("", &result);

        // update balance proof
        balance_proof = Some(
            balance_processor
                .prove_send(
                    &validity_processor.get_verifier_data(),
                    key.pubkey,
                    &tx_witness,
                    &update_witness,
                    &spent_proof,
                    &balance_proof,
                )
                .unwrap(),
        );
    }

    // update recipient's balance proof
    {
        let update_witness = validity_state_manager
            .get_update_witness(recipient.pubkey, sent_block_number, 0, false)
            .unwrap();
        recipient_balance_proof = Some(
            balance_processor
                .prove_update(
                    &validity_processor.get_verifier_data(),
                    recipient.pubkey,
                    &update_witness,
                    &recipient_balance_proof,
                )
                .unwrap(),
        );
    }

    // receive transfer
    {
        let transfer_witness = transfer_witnesses[0].clone();
        let nullifier: Bytes32 = transfer_witness.transfer.commitment().into();
        let private_transition_witness = PrivateTransitionWitness::new(
            &mut recipient_private_state,
            transfer_witness.transfer.token_index,
            transfer_witness.transfer.amount,
            nullifier,
            Salt::rand(&mut rng),
        )
        .unwrap();
        let block_merkle_proof = validity_state_manager
            .get_block_merkle_proof(sent_block_number, sent_block_number)
            .unwrap();
        let receive_transfer_witness = ReceiveTransferWitness {
            transfer_witness,
            private_transition_witness,
            sender_balance_proof: balance_proof.clone().unwrap(), // sender balance proof
            block_merkle_proof,
        };
        let request = ProveReceiveTransferRequest {
            pubkey: recipient.pubkey,
            receive_transfer_witness: receive_transfer_witness.clone(),
            prev_proof: recipient_balance_proof.clone(),
        };
        let request = ProveRequestWithType {
            prove_type: ProveType::ReceiveTransfer,
            pubkey: key.pubkey, // use original pubkey for integrity
            request: bincode::serialize(&request).unwrap(),
        };
        save_proof_request("", &request);
    }

    // withdrawal
    {
        let transfer_witness = transfer_witnesses[1].clone();
        let withdrawal_witness = WithdrawalWitness {
            transfer_witness,
            balance_proof: balance_proof.clone().unwrap(),
        };
        let request = ProveSingleWithdrawalRequest { withdrawal_witness };
        let request = ProveRequestWithType {
            prove_type: ProveType::SingleWithdrawal,
            pubkey: key.pubkey,
            request: bincode::serialize(&request).unwrap(),
        };
        save_proof_request("", &request);
    }
}

fn save_proof_request(prefix: &str, request: &ProveRequestWithType) {
    let path = PathBuf::from_str("tests/test_data/private_zkp_server").unwrap();
    fs::create_dir_all(&path).unwrap();
    let file_name = format!("{}{}.json", prefix, request.prove_type);
    let file_path = path.join(file_name);
    let json = serde_json::to_string_pretty(request).unwrap();
    std::fs::write(file_path, json).unwrap();
}
