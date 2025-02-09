use intmax2_interfaces::{
    api::{
        balance_prover::interface::BalanceProverClientInterface,
        block_builder::interface::{CollateralBlock, Fee, FeeInfo, FeeProof},
        store_vault_server::interface::StoreVaultClientInterface,
    },
    data::{
        encryption::Encryption, proof_compression::CompressedSpentProof,
        sender_proof_set::SenderProofSet, transfer_data::TransferData, user_data::UserData,
    },
};
use intmax2_zkp::{
    common::{
        signature::{
            flatten::FlatG2,
            key_set::KeySet,
            sign::{get_pubkey_hash, sign_to_tx_root_and_expiry},
        },
        transfer::Transfer,
        trees::{transfer_tree::TransferTree, tx_tree::TxTree},
        tx::Tx,
        witness::{spent_witness::SpentWitness, transfer_witness::TransferWitness},
    },
    constants::{NUM_SENDERS_IN_BLOCK, TRANSFER_TREE_HEIGHT, TX_TREE_HEIGHT},
    ethereum_types::{bytes32::Bytes32, u256::U256},
};
use num_bigint::BigUint;

use super::{error::ClientError, sync::utils::generate_spent_witness};

#[allow(clippy::too_many_arguments)]
pub async fn generate_fee_proof<S: StoreVaultClientInterface, B: BalanceProverClientInterface>(
    store_vault_server: &S,
    balance_prover: &B,
    key: KeySet,
    user_data: &UserData,
    sender_proof_set_ephemeral_key: U256,
    tx_nonce: u32,
    fee_index: u32,
    transfers: &[Transfer],
    collateral_transfer: Option<Transfer>,
) -> Result<(FeeProof, Option<SpentWitness>), ClientError> {
    let mut transfer_tree = TransferTree::new(TRANSFER_TREE_HEIGHT);
    for transfer in transfers {
        transfer_tree.push(*transfer);
    }
    let tx = Tx {
        transfer_tree_root: transfer_tree.get_root(),
        nonce: tx_nonce,
    };
    let fee_transfer_witness = TransferWitness {
        tx,
        transfer: transfers[fee_index as usize],
        transfer_index: fee_index,
        transfer_merkle_proof: transfer_tree.prove(fee_index as u64),
    };
    let (collateral_block, collateral_spent_witness) =
        if let Some(collateral_transfer) = collateral_transfer {
            // spent proof
            let transfers = vec![collateral_transfer];
            let collateral_spent_witness =
                generate_spent_witness(&user_data.full_private_state, tx_nonce, &transfers).await?;
            let tx = collateral_spent_witness.tx;
            let spent_proof = balance_prover
                .prove_spent(key, &collateral_spent_witness)
                .await?;
            let compressed_spent_proof = CompressedSpentProof::new(&spent_proof)?;
            let sender_proof_set = SenderProofSet {
                spent_proof: compressed_spent_proof,
                prev_balance_proof: user_data.balance_proof.clone().unwrap(), // unwrap is safe
            };
            let ephemeral_key = KeySet::rand(&mut rand::thread_rng());
            store_vault_server
                .save_sender_proof_set(
                    ephemeral_key,
                    &sender_proof_set.encrypt(ephemeral_key.pubkey),
                )
                .await?;
            let sender_proof_set_ephemeral_key: U256 =
                BigUint::from(ephemeral_key.privkey).try_into().unwrap();

            let mut transfer_tree = TransferTree::new(TRANSFER_TREE_HEIGHT);
            transfer_tree.push(collateral_transfer);
            let transfer_index = 0u32;
            let transfer_merkle_proof = transfer_tree.prove(transfer_index as u64);
            let mut tx_tree = TxTree::new(TX_TREE_HEIGHT);
            tx_tree.push(tx);
            let tx_index = 0u32;
            let tx_merkle_proof = tx_tree.prove(tx_index as u64);
            let tx_tree_root: Bytes32 = tx_tree.get_root().into();
            let mut pubkeys = vec![key.pubkey];
            pubkeys.resize(NUM_SENDERS_IN_BLOCK, U256::dummy_pubkey());
            let pubkey_hash = get_pubkey_hash(&pubkeys);

            let fee_transfer_data = TransferData {
                sender_proof_set_ephemeral_key,
                sender_proof_set: None,
                sender: key.pubkey,
                tx,
                tx_index,
                tx_merkle_proof,
                tx_tree_root,
                transfer: collateral_transfer,
                transfer_index,
                transfer_merkle_proof,
            };

            let expiry = 0; // todo: set expiry
            let signature: FlatG2 =
                sign_to_tx_root_and_expiry(key.privkey, tx_tree_root, expiry, pubkey_hash).into();
            let collateral_block = CollateralBlock {
                sender_proof_set_ephemeral_key,
                fee_transfer_data,
                expiry,
                signature,
            };
            (Some(collateral_block), Some(collateral_spent_witness))
        } else {
            (None, None)
        };
    let fee_proof = FeeProof {
        fee_transfer_witness,
        collateral_block,
        sender_proof_set_ephemeral_key,
    };

    Ok((fee_proof, collateral_spent_witness))
}

pub fn quote_fee(
    is_registration_block: bool,
    fee_index: u32,
    fee_info: &FeeInfo,
) -> Result<(U256, U256), ClientError> {
    let fee_list = if is_registration_block {
        &fee_info.registration_fee
    } else {
        &fee_info.non_registration_fee
    };
    let fee = if fee_list.is_some() {
        get_fee(fee_index, fee_list.as_ref().unwrap())?
    } else {
        U256::default()
    };
    let collateral_fee_list = if is_registration_block {
        &fee_info.registration_collateral_fee
    } else {
        &fee_info.non_registration_collateral_fee
    };
    let collateral_fee = if collateral_fee_list.is_some() {
        get_fee(fee_index, collateral_fee_list.as_ref().unwrap())?
    } else {
        U256::default()
    };
    Ok((fee, collateral_fee))
}

fn get_fee(fee_index: u32, fee_list: &[Fee]) -> Result<U256, ClientError> {
    fee_list
        .get(fee_index as usize)
        .map(|fee| fee.amount)
        .ok_or_else(|| ClientError::BlockBuilderFeeError("Fee token is not found".to_string()))
}
