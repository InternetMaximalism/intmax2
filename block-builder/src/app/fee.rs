use super::{block_post::BlockPostTask, error::FeeError, types::ProposalMemo};
use intmax2_client_sdk::client::strategy::common::fetch_sender_proof_set;
use intmax2_interfaces::{
    api::{
        block_builder::interface::FeeProof,
        store_vault_server::interface::{SaveDataEntry, StoreVaultClientInterface},
    },
    data::{
        data_type::DataType, encryption::BlsEncryption, extra_data::ExtraData,
        sender_proof_set::SenderProofSet, transfer_data::TransferData, validation::Validation,
    },
    utils::{
        address::IntmaxAddress,
        fee::Fee,
        key::{PrivateKey, PublicKey},
        random::default_rng,
    },
};
use intmax2_zkp::{
    circuits::balance::send::spent_circuit::SpentPublicInputs,
    common::{
        block_builder::UserSignature,
        signature_content::{block_sign_payload::BlockSignPayload, utils::get_pubkey_hash},
        witness::transfer_witness::TransferWitness,
    },
    constants::NUM_SENDERS_IN_BLOCK,
    ethereum_types::{
        account_id::{AccountId, AccountIdPacked},
        address::Address,
        bytes32::Bytes32,
        u256::U256,
    },
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Validate fee proof
pub async fn validate_fee_proof(
    store_vault_server_client: &dyn StoreVaultClientInterface,
    beneficiary: IntmaxAddress,
    block_builder_address: Address,
    required_fee: Option<&Vec<Fee>>,
    required_collateral_fee: Option<&Vec<Fee>>,
    sender_spend_pub: PublicKey,
    fee_proof: &Option<FeeProof>,
) -> Result<(), FeeError> {
    log::info!(
        "validate_fee_proof: required_fee {}, required_collateral_fee {}",
        required_fee.is_some(),
        required_collateral_fee.is_some()
    );
    let required_fee = match required_fee {
        Some(fee) => fee,
        None => return Ok(()),
    };

    let fee_proof = fee_proof
        .as_ref()
        .ok_or_else(|| FeeError::InvalidFee("Fee proof is missing".to_string()))?;
    let sender_proof_set = fetch_sender_proof_set(
        store_vault_server_client,
        fee_proof.sender_proof_set_ephemeral_key,
    )
    .await?;

    // validate main fee
    validate_fee_single(
        beneficiary,
        required_fee,
        &sender_proof_set,
        &fee_proof.fee_transfer_witness,
    )
    .await?;

    // validate collateral fee
    if let Some(collateral_fee) = required_collateral_fee {
        let collateral_block = fee_proof.collateral_block.as_ref().ok_or_else(|| {
            FeeError::FeeVerificationError("Collateral block is missing".to_string())
        })?;
        // validate block builder address
        if collateral_block.block_builder_address != block_builder_address {
            return Err(FeeError::FeeVerificationError(
                "Invalid block builder address in collateral block".to_string(),
            ));
        }
        // validate transfer data
        let transfer_data = &collateral_block.fee_transfer_data;
        transfer_data.validate().map_err(|e| {
            log::error!("Failed to validate transfer data: {e}");
            FeeError::FeeVerificationError("Failed to validate transfer data".to_string())
        })?;

        // validate signature
        let user_signature = UserSignature {
            pubkey: sender_spend_pub.0,
            signature: collateral_block.signature.clone(),
        };
        let pubkeys = padded_pubkeys(sender_spend_pub.0);
        let pubkey_hash = get_pubkey_hash(&pubkeys);
        let block_sign_payload = BlockSignPayload {
            is_registration_block: collateral_block.is_registration_block,
            tx_tree_root: transfer_data.tx_tree_root,
            expiry: collateral_block.expiry.into(),
            block_builder_address,
            block_builder_nonce: 0,
        };
        verify_user_signature(&user_signature, &block_sign_payload, pubkey_hash)?;

        let sender_proof_set = fetch_sender_proof_set(
            store_vault_server_client,
            collateral_block.sender_proof_set_ephemeral_key,
        )
        .await?;

        let transfer_witness = TransferWitness {
            tx: transfer_data.tx,
            transfer: transfer_data.transfer,
            transfer_index: transfer_data.transfer_index,
            transfer_merkle_proof: transfer_data.transfer_merkle_proof.clone(),
        };
        validate_fee_single(
            beneficiary,
            collateral_fee,
            &sender_proof_set,
            &transfer_witness,
        )
        .await?;
    }
    Ok(())
}

/// common function to validate fee
async fn validate_fee_single(
    beneficiary: IntmaxAddress,
    required_fee: &[Fee], // token index -> fee amount
    sender_proof_set: &SenderProofSet,
    transfer_witness: &TransferWitness,
) -> Result<(), FeeError> {
    sender_proof_set.validate().map_err(|e| {
        FeeError::FeeVerificationError(format!("Failed to validate sender proof set: {e}"))
    })?;

    // validate spent proof pis
    let spent_proof = sender_proof_set.spent_proof.decompress()?;
    let spent_pis = SpentPublicInputs::from_pis(&spent_proof.public_inputs).map_err(|e| {
        FeeError::FeeVerificationError(format!("Failed to decompress spent proof: {e}"))
    })?;
    if spent_pis.tx != transfer_witness.tx {
        return Err(FeeError::FeeVerificationError(
            "Tx in spent proof is not the same as transfer witness tx".to_string(),
        ));
    }
    let insufficient_flag = spent_pis
        .insufficient_flags
        .random_access(transfer_witness.transfer_index as usize);
    if insufficient_flag {
        return Err(FeeError::FeeVerificationError(
            "Insufficient flag is on in spent proof".to_string(),
        ));
    }

    // validate transfer witness
    transfer_witness
        .transfer_merkle_proof
        .verify(
            &transfer_witness.transfer,
            transfer_witness.transfer_index as u64,
            transfer_witness.tx.transfer_tree_root,
        )
        .map_err(|e| {
            FeeError::MerkleTreeError(format!("Failed to verify transfer merkle proof: {e}"))
        })?;

    // make sure that transfer is for beneficiary account
    let recipient = transfer_witness.transfer.recipient;
    if !recipient.is_pubkey {
        return Err(FeeError::InvalidRecipient(
            "Recipient is not a pubkey".to_string(),
        ));
    }
    let recipient = recipient.to_pubkey().unwrap();
    if recipient != beneficiary.public_spend.0 {
        return Err(FeeError::InvalidRecipient(
            "Recipient is not the beneficiary".to_string(),
        ));
    }

    // make sure that the fee is correct

    let requested_fee = required_fee
        .iter()
        .find(|f| f.token_index == transfer_witness.transfer.token_index)
        .ok_or_else(|| {
            FeeError::InvalidFee(format!(
                "Fee for token index {} is not found",
                transfer_witness.transfer.token_index
            ))
        })?;
    if transfer_witness.transfer.amount < requested_fee.amount {
        return Err(FeeError::InvalidFee(format!(
            "Transfer amount is not enough: requested_fee: {}, transfer_amount: {}",
            requested_fee.amount, transfer_witness.transfer.amount
        )));
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeeCollection {
    pub use_collateral: bool,
    pub memo: ProposalMemo,
    pub signatures: Vec<UserSignature>,
}

/// Collect fee from the senders
pub async fn collect_fee(
    store_vault_server_client: &dyn StoreVaultClientInterface,
    beneficiary: IntmaxAddress,
    fee_collection: &FeeCollection,
) -> Result<Vec<BlockPostTask>, FeeError> {
    let mut block_post_tasks = Vec::new();

    log::info!(
        "collect_fee: use_collateral {}",
        fee_collection.use_collateral
    );
    let mut transfer_data_vec = Vec::new();
    let memo = &fee_collection.memo;
    for (request, proposal) in memo.tx_requests.iter().zip(memo.proposals.iter()) {
        // this already validated in the tx request phase
        let fee_proof = request
            .fee_proof
            .as_ref()
            .ok_or_else(|| FeeError::InvalidFee("Fee proof is missing".to_string()))?;

        // check if the sender returned the signature
        let signature = fee_collection
            .signatures
            .iter()
            .find(|s| s.pubkey == request.spend_pub());
        if signature.is_some() {
            // fee will be paid
            let transfer_data = TransferData {
                sender_proof_set_ephemeral_key: fee_proof.sender_proof_set_ephemeral_key,
                sender_proof_set: None,
                sender: request.sender,
                extra_data: ExtraData::default(),
                tx: request.tx,
                tx_index: proposal.tx_index,
                tx_merkle_proof: proposal.tx_merkle_proof.clone(),
                tx_tree_root: proposal.block_sign_payload.tx_tree_root,
                transfer: fee_proof.fee_transfer_witness.transfer,
                transfer_index: fee_proof.fee_transfer_witness.transfer_index,
                transfer_merkle_proof: fee_proof.fee_transfer_witness.transfer_merkle_proof.clone(),
            };
            transfer_data_vec.push(transfer_data);
            log::info!("sender {}'s fee is collected", request.spend_pub());
        } else {
            if !fee_collection.use_collateral {
                log::warn!(
                    "sender {} did not return the signature for the fee but collateral is not enabled",
                    request.spend_pub()
                );
                continue;
            }

            // this is already validated in the tx request phase
            let collateral_block = fee_proof
                .collateral_block
                .as_ref()
                .ok_or_else(|| FeeError::InvalidFee("Collateral block is missing".to_string()))?;

            let transfer_data = &collateral_block.fee_transfer_data;
            let pubkeys = padded_pubkeys(request.spend_pub());
            let pubkey_hash = get_pubkey_hash(&pubkeys);
            let account_ids = request.account_id.map(|id| {
                let account_ids = padded_account_ids(id);
                AccountIdPacked::pack(&account_ids)
            });
            let signature = UserSignature {
                pubkey: request.spend_pub(),
                signature: collateral_block.signature.clone(),
            };
            let block_sign_payload = BlockSignPayload {
                is_registration_block: collateral_block.is_registration_block,
                tx_tree_root: transfer_data.tx_tree_root,
                expiry: collateral_block.expiry.into(),
                block_builder_address: collateral_block.block_builder_address,
                block_builder_nonce: 0,
            };
            // validate signature again
            verify_user_signature(&signature, &block_sign_payload, pubkey_hash)?;

            // save transfer data
            transfer_data_vec.push(transfer_data.clone());

            let block_post = BlockPostTask {
                force_post: false,
                block_sign_payload,
                pubkeys,
                account_ids,
                pubkey_hash,
                signatures: vec![signature],
                block_id: Uuid::new_v4().to_string(),
            };
            block_post_tasks.push(block_post);
            log::warn!(
                "sender {}'s collateral block is queued",
                request.spend_pub()
            );
        }
    }

    if transfer_data_vec.is_empty() {
        // early return if no fee to collect
        return Ok(block_post_tasks);
    }

    // save transfer data to the store vault server
    let mut entries = Vec::new();
    for transfer_data in transfer_data_vec {
        let entry = SaveDataEntry {
            topic: DataType::Transfer.to_topic(),
            pubkey: beneficiary.public_view.0,
            data: transfer_data.encrypt(beneficiary.public_view, None)?,
        };
        entries.push(entry);
    }
    let dummy_key = PrivateKey::rand(&mut default_rng());
    let _digests = store_vault_server_client
        .save_data_batch(dummy_key, &entries)
        .await?;
    Ok(block_post_tasks)
}

fn padded_pubkeys(pubkey: U256) -> Vec<U256> {
    let mut keys = vec![pubkey];
    keys.resize(NUM_SENDERS_IN_BLOCK, U256::dummy_pubkey());
    keys
}

fn padded_account_ids(account_id: AccountId) -> Vec<AccountId> {
    let mut accounts = vec![account_id];
    accounts.resize(NUM_SENDERS_IN_BLOCK, AccountId::dummy());
    accounts
}

fn verify_user_signature(
    signature: &UserSignature,
    payload: &BlockSignPayload,
    pubkey_hash: Bytes32,
) -> Result<(), FeeError> {
    signature.verify(payload, pubkey_hash).map_err(|e| {
        FeeError::SignatureVerificationError(format!("Failed to verify signature: {e}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use intmax2_zkp::ethereum_types::{account_id::ACCOUNT_ID_BYTES_LEN, u256::U256};
    fn account_id_from_u64(val: u64) -> AccountId {
        let bytes = val.to_be_bytes();
        AccountId::from_bytes_be(&bytes[8 - ACCOUNT_ID_BYTES_LEN..])
    }

    #[test]
    fn test_padded_pubkeys_exact_size() {
        let pubkey = U256::from(12345);
        let result = padded_pubkeys(pubkey);

        assert_eq!(result.len(), NUM_SENDERS_IN_BLOCK);
        assert_eq!(result[0], pubkey);

        for (i, key) in result.iter().enumerate().skip(1) {
            assert_eq!(*key, U256::dummy_pubkey(), "Mismatch at index {i}")
        }
    }

    #[test]
    fn test_padded_account_ids_exact_size() {
        let account = account_id_from_u64(42);
        let result1 = padded_account_ids(account);

        assert_eq!(result1.len(), NUM_SENDERS_IN_BLOCK);
        assert_eq!(result1[0], account);

        for (i, id) in result1.iter().enumerate().skip(1) {
            assert_eq!(*id, AccountId::dummy(), "Mismatch at index {i}");
        }
    }

    #[test]
    fn test_padded_pubkeys_idempotent() {
        let pubkey = U256::from(0xdeadbeef);
        let result1 = padded_pubkeys(pubkey);
        let result2 = padded_pubkeys(pubkey);
        assert_eq!(result1, result2);
    }

    #[test]
    fn test_padded_account_ids_idempotent() {
        let account = account_id_from_u64(1);
        let result1 = padded_account_ids(account);
        let result2 = padded_account_ids(account);

        assert_eq!(result1, result2);
    }
}
