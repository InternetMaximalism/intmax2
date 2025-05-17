use intmax2_interfaces::{
    api::{
        balance_prover::interface::BalanceProverClientInterface,
        block_builder::interface::{BlockBuilderFeeInfo, CollateralBlock, Fee, FeeProof},
        store_vault_server::interface::{SaveDataEntry, StoreVaultClientInterface},
    },
    data::{
        data_type::DataType, encryption::BlsEncryption, proof_compression::CompressedSpentProof,
        sender_proof_set::SenderProofSet, transfer_data::TransferData, tx_data::TxData,
        user_data::UserData,
    },
    utils::random::default_rng,
};
use intmax2_zkp::{
    common::{
        signature_content::{
            block_sign_payload::BlockSignPayload, key_set::KeySet, utils::get_pubkey_hash,
        },
        transfer::Transfer,
        trees::{transfer_tree::TransferTree, tx_tree::TxTree},
        tx::Tx,
        witness::transfer_witness::TransferWitness,
    },
    constants::{NUM_SENDERS_IN_BLOCK, TRANSFER_TREE_HEIGHT, TX_TREE_HEIGHT},
    ethereum_types::{address::Address, bytes32::Bytes32, u256::U256},
};

use super::{error::ClientError, sync::utils::generate_spent_witness};

#[allow(clippy::too_many_arguments)]
pub async fn generate_fee_proof(
    store_vault_server: &dyn StoreVaultClientInterface,
    balance_prover: &dyn BalanceProverClientInterface,
    tx_timeout: u64,
    key: KeySet,
    user_data: &UserData,
    sender_proof_set_ephemeral_key: U256,
    tx_nonce: u32,
    fee_index: u32,
    transfers: &[Transfer],
    collateral_transfer: Option<Transfer>,
    is_registration_block: bool,
    block_builder_address: Address,
) -> Result<FeeProof, ClientError> {
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
    let collateral_block = if let Some(collateral_transfer) = collateral_transfer {
        // spent proof
        let transfers = vec![collateral_transfer];
        let collateral_spent_witness =
            generate_spent_witness(&user_data.full_private_state, tx_nonce, &transfers)?;
        let tx = collateral_spent_witness.tx;
        let spent_proof = balance_prover
            .prove_spent(key, &collateral_spent_witness)
            .await?;
        let compressed_spent_proof = CompressedSpentProof::new(&spent_proof)?;
        let sender_proof_set = SenderProofSet {
            spent_proof: compressed_spent_proof,
            prev_balance_proof: user_data.balance_proof.clone().unwrap(), // unwrap is safe
        };
        let ephemeral_key = KeySet::rand(&mut default_rng());
        store_vault_server
            .save_snapshot(
                ephemeral_key,
                &DataType::SenderProofSet.to_topic(),
                None,
                &sender_proof_set.encrypt(ephemeral_key.pubkey, Some(ephemeral_key))?,
            )
            .await?;
        let sender_proof_set_ephemeral_key = ephemeral_key.privkey;

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

        let expiry = tx_timeout + chrono::Utc::now().timestamp() as u64;
        let block_sign_payload = BlockSignPayload {
            is_registration_block,
            tx_tree_root,
            expiry: expiry.into(),
            block_builder_address,
            block_builder_nonce: 0, // contract will ignore nonce checking
        };
        let signature = block_sign_payload.sign(key.privkey, pubkey_hash);
        let collateral_block = CollateralBlock {
            sender_proof_set_ephemeral_key,
            fee_transfer_data,
            is_registration_block,
            expiry,
            block_builder_address,
            signature,
        };

        // save tx data for collateral block
        let transfer_data = &collateral_block.fee_transfer_data;
        let tx_data = TxData {
            tx_index: transfer_data.tx_index,
            tx_merkle_proof: transfer_data.tx_merkle_proof.clone(),
            tx_tree_root: transfer_data.tx_tree_root,
            spent_witness: collateral_spent_witness.clone(),
            sender_proof_set_ephemeral_key: collateral_block.sender_proof_set_ephemeral_key,
        };
        let entry = SaveDataEntry {
            topic: DataType::Tx.to_topic(),
            pubkey: key.pubkey,
            data: tx_data.encrypt(key.pubkey, Some(key))?,
        };
        store_vault_server.save_data_batch(key, &[entry]).await?;

        Some(collateral_block)
    } else {
        None
    };

    Ok(FeeProof {
        fee_transfer_witness,
        collateral_block,
        sender_proof_set_ephemeral_key,
    })
}

pub(crate) fn quote_transfer_fee(
    is_registration_block: bool,
    fee_token_index: u32,
    fee_info: &BlockBuilderFeeInfo,
) -> Result<(Option<Fee>, Option<Fee>), ClientError> {
    let fee_list = if is_registration_block {
        &fee_info.registration_fee
    } else {
        &fee_info.non_registration_fee
    };
    let fee = fee_list
        .as_ref()
        .map(|fee_list| get_fee(fee_token_index, fee_list))
        .transpose()?;

    let collateral_fee_list = if is_registration_block {
        &fee_info.registration_collateral_fee
    } else {
        &fee_info.non_registration_collateral_fee
    };
    let collateral_fee = collateral_fee_list
        .as_ref()
        .map(|collateral_fee_list| get_fee(fee_token_index, collateral_fee_list))
        .transpose()?;
    Ok((fee, collateral_fee))
}

fn get_fee(fee_token_index: u32, fee_list: &[Fee]) -> Result<Fee, ClientError> {
    let fee = fee_list
        .iter()
        .find(|fee| fee.token_index == fee_token_index)
        .ok_or(ClientError::BlockBuilderFeeError(
            "Fee not found".to_string(),
        ))?;
    Ok(fee.clone())
}
