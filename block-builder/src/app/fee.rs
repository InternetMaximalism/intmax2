use std::collections::HashMap;

use super::{block_post::BlockPostTask, error::FeeError, types::ProposalMemo};
use intmax2_client_sdk::client::strategy::common::fetch_sender_proof_set;
use intmax2_interfaces::{
    api::{
        block_builder::interface::{Fee, FeeProof},
        store_vault_server::interface::{SaveDataEntry, StoreVaultClientInterface},
    },
    data::{
        data_type::DataType, encryption::BlsEncryption, sender_proof_set::SenderProofSet,
        transfer_data::TransferData, validation::Validation,
    },
    utils::random::default_rng,
};
use intmax2_zkp::{
    circuits::balance::send::spent_circuit::SpentPublicInputs,
    common::{
        block_builder::UserSignature,
        signature_content::{
            block_sign_payload::BlockSignPayload, key_set::KeySet, utils::get_pubkey_hash,
        },
        witness::transfer_witness::TransferWitness,
    },
    constants::NUM_SENDERS_IN_BLOCK,
    ethereum_types::{
        account_id::{AccountId, AccountIdPacked},
        address::Address,
        u256::U256,
    },
};
use num_bigint::BigUint;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Validate fee proof
pub async fn validate_fee_proof(
    store_vault_server_client: &dyn StoreVaultClientInterface,
    beneficiary_pubkey: Option<U256>,
    block_builder_address: Address,
    required_fee: Option<&HashMap<u32, U256>>,
    required_collateral_fee: Option<&HashMap<u32, U256>>,
    sender: U256,
    fee_proof: &Option<FeeProof>,
) -> Result<(), FeeError> {
    log::info!(
        "validate_fee_proof: required_fee {}, required_collateral_fee {}",
        required_fee.is_some(),
        required_collateral_fee.is_some()
    );
    if required_fee.is_none() {
        return Ok(());
    }
    let required_fee = required_fee.unwrap();
    let fee_proof = fee_proof
        .as_ref()
        .ok_or(FeeError::InvalidFee("Fee proof is missing".to_string()))?;
    let beneficiary_pubkey = beneficiary_pubkey.ok_or(FeeError::InvalidFee(
        "Beneficiary pubkey is missing".to_string(),
    ))?;

    let sender_proof_set = fetch_sender_proof_set(
        store_vault_server_client,
        fee_proof.sender_proof_set_ephemeral_key,
    )
    .await?;

    // validate main fee
    validate_fee_single(
        beneficiary_pubkey,
        required_fee,
        &sender_proof_set,
        &fee_proof.fee_transfer_witness,
    )
    .await?;

    // validate collateral fee
    if let Some(collateral_fee) = required_collateral_fee {
        let collateral_block =
            fee_proof
                .collateral_block
                .as_ref()
                .ok_or(FeeError::FeeVerificationError(
                    "Collateral block is missing".to_string(),
                ))?;
        // validate transfer data
        let transfer_data = &collateral_block.fee_transfer_data;
        match transfer_data.validate(U256::dummy_pubkey()) {
            Ok(_) => {}
            Err(e) => {
                log::error!("Failed to validate transfer data: {}", e);
                return Err(FeeError::FeeVerificationError(
                    "Failed to validate transfer data".to_string(),
                ));
            }
        }
        if collateral_block.block_builder_address != block_builder_address {
            return Err(FeeError::FeeVerificationError(
                "Invalid block builder address in collateral block".to_string(),
            ));
        }

        // validate signature
        let user_signature = UserSignature {
            pubkey: sender,
            signature: collateral_block.signature.clone(),
        };
        let mut pubkeys = vec![sender];
        pubkeys.resize(NUM_SENDERS_IN_BLOCK, U256::dummy_pubkey());
        let pubkey_hash = get_pubkey_hash(&pubkeys);
        let block_sign_payload = BlockSignPayload {
            is_registration_block: collateral_block.is_registration_block,
            tx_tree_root: transfer_data.tx_tree_root,
            expiry: collateral_block.expiry.into(),
            block_builder_address,
            block_builder_nonce: 0,
        };
        user_signature
            .verify(&block_sign_payload, pubkey_hash)
            .map_err(|e| {
                FeeError::SignatureVerificationError(format!("Failed to verify signature: {}", e))
            })?;
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
            beneficiary_pubkey,
            collateral_fee,
            &sender_proof_set,
            &transfer_witness,
        )
        .await?;
    }
    Ok(())
}

/// common function to validate fee and collateral fee
async fn validate_fee_single(
    beneficiary_pubkey: U256,
    required_fee: &HashMap<u32, U256>, // token index -> fee amount
    sender_proof_set: &SenderProofSet,
    transfer_witness: &TransferWitness,
) -> Result<(), FeeError> {
    sender_proof_set
        .validate(U256::dummy_pubkey())
        .map_err(|e| {
            FeeError::FeeVerificationError(format!("Failed to validate sender proof set: {}", e))
        })?;

    // validate spent proof pis
    let spent_proof = sender_proof_set.spent_proof.decompress()?;
    let spent_pis = SpentPublicInputs::from_pis(&spent_proof.public_inputs).map_err(|e| {
        FeeError::FeeVerificationError(format!("Failed to decompress spent proof: {}", e))
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
            FeeError::MerkleTreeError(format!("Failed to verify transfer merkle proof: {}", e))
        })?;

    // make sure that transfer is for beneficiary account
    let recipient = transfer_witness.transfer.recipient;
    if !recipient.is_pubkey {
        return Err(FeeError::InvalidRecipient(
            "Recipient is not a pubkey".to_string(),
        ));
    }
    let recipient = recipient.to_pubkey().unwrap();
    if recipient != beneficiary_pubkey {
        return Err(FeeError::InvalidRecipient(
            "Recipient is not the beneficiary".to_string(),
        ));
    }

    // make sure that the fee is correct
    if !required_fee.contains_key(&transfer_witness.transfer.token_index) {
        return Err(FeeError::InvalidFee(
            "Fee token index is not correct".to_string(),
        ));
    }
    let requested_fee = required_fee
        .get(&transfer_witness.transfer.token_index)
        .unwrap();
    if transfer_witness.transfer.amount < *requested_fee {
        return Err(FeeError::InvalidFee(format!(
            "Transfer amount is not enough: requested_fee: {}, transfer_amount: {}",
            requested_fee, transfer_witness.transfer.amount
        )));
    }
    Ok(())
}

/// Parse fee string into a map of token index -> fee amount
// Example: "0:100,1:200" -> {0: 100, 1: 200}
pub fn parse_fee_str(fee: &str) -> Result<HashMap<u32, U256>, FeeError> {
    let mut fee_map = HashMap::new();
    for fee_str in fee.split(',') {
        let fee_parts: Vec<&str> = fee_str.split(':').collect();
        if fee_parts.len() != 2 {
            return Err(FeeError::ParseError(
                "Invalid fee format: should be token_index:fee_amount".to_string(),
            ));
        }
        let token_index = fee_parts[0]
            .parse::<u32>()
            .map_err(|e| FeeError::ParseError(format!("Failed to parse token index: {}", e)))?;
        let fee_amount: U256 = fee_parts[1]
            .parse::<BigUint>()
            .map_err(|e| FeeError::ParseError(format!("Failed to parse fee amount: {}", e)))?
            .try_into()
            .map_err(|e| FeeError::ParseError(format!("Failed to convert fee amount: {}", e)))?;
        fee_map.insert(token_index, fee_amount);
    }
    Ok(fee_map)
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
    beneficiary_pubkey: U256,
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
            .ok_or(FeeError::InvalidFee("Fee proof is missing".to_string()))?;

        // check if the sender returned the signature
        let signature = fee_collection
            .signatures
            .iter()
            .find(|s| s.pubkey == request.pubkey);
        if signature.is_some() {
            // fee will be paid
            let transfer_data = TransferData {
                sender_proof_set_ephemeral_key: fee_proof.sender_proof_set_ephemeral_key,
                sender_proof_set: None,
                sender: request.pubkey,
                tx: request.tx,
                tx_index: proposal.tx_index,
                tx_merkle_proof: proposal.tx_merkle_proof.clone(),
                tx_tree_root: proposal.block_sign_payload.tx_tree_root,
                transfer: fee_proof.fee_transfer_witness.transfer,
                transfer_index: fee_proof.fee_transfer_witness.transfer_index,
                transfer_merkle_proof: fee_proof.fee_transfer_witness.transfer_merkle_proof.clone(),
            };
            transfer_data_vec.push(transfer_data);
            log::info!("sender {}'s fee is collected", request.pubkey);
        } else {
            if !fee_collection.use_collateral {
                log::warn!(
                    "sender {} did not return the signature for the fee but collateral is not enabled",
                    request.pubkey
                );
                continue;
            }
            // this is already validated in the tx request phase
            let collateral_block =
                fee_proof
                    .collateral_block
                    .as_ref()
                    .ok_or(FeeError::InvalidFee(
                        "Collateral block is missing".to_string(),
                    ))?;

            let transfer_data = &collateral_block.fee_transfer_data;
            let mut pubkeys = vec![request.pubkey];
            pubkeys.resize(NUM_SENDERS_IN_BLOCK, U256::dummy_pubkey());
            let pubkey_hash = get_pubkey_hash(&pubkeys);
            let account_ids = request.account_id.map(|id| {
                let mut account_ids = vec![id];
                account_ids.resize(NUM_SENDERS_IN_BLOCK, AccountId::dummy());
                AccountIdPacked::pack(&account_ids)
            });
            let signature = UserSignature {
                pubkey: request.pubkey,
                signature: collateral_block.signature.clone(),
            };
            let block_sign_payload = BlockSignPayload {
                is_registration_block: collateral_block.is_registration_block,
                tx_tree_root: transfer_data.tx_tree_root,
                expiry: collateral_block.expiry.into(),
                block_builder_address: collateral_block.block_builder_address, // todo: check address
                block_builder_nonce: 0,
            };
            // validate signature again
            signature
                .verify(&block_sign_payload, pubkey_hash)
                .map_err(|e| {
                    FeeError::SignatureVerificationError(format!(
                        "Failed to verify signature: {}",
                        e
                    ))
                })?;

            // save transfer data
            transfer_data_vec.push(transfer_data.clone());

            let block_post = BlockPostTask {
                force_post: false,
                block_sign_payload: memo.block_sign_payload.clone(),
                pubkeys,
                account_ids,
                pubkey_hash,
                signatures: vec![signature],
                block_id: Uuid::new_v4().to_string(),
            };
            block_post_tasks.push(block_post);
            log::warn!("sender {}'s collateral block is queued", request.pubkey);
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
            pubkey: beneficiary_pubkey,
            data: transfer_data.encrypt(beneficiary_pubkey, None)?,
        };
        entries.push(entry);
    }
    let dummy_key = KeySet::rand(&mut default_rng());
    let _digests = store_vault_server_client
        .save_data_batch(dummy_key, &entries)
        .await?;
    Ok(block_post_tasks)
}

pub fn convert_fee_vec(fee: &Option<HashMap<u32, U256>>) -> Option<Vec<Fee>> {
    fee.as_ref().map(|fee| {
        fee.iter()
            .map(|(token_index, amount)| Fee {
                token_index: *token_index,
                amount: *amount,
            })
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use intmax2_zkp::ethereum_types::u256::U256;
    use num_bigint::BigUint;
    use num_traits::One;
    use std::collections::HashMap;

    #[test]
    fn test_fee_amount_zero() {
        let fee_str = "0:0";
        let result = parse_fee_str(fee_str).unwrap();

        let mut expected = HashMap::new();
        expected.insert(0, U256::default());

        assert_eq!(result, expected);
    }

    #[test]
    fn test_few_fees() {
        let fee_str = "0:100,1:200";
        let result = parse_fee_str(fee_str).unwrap();

        let mut expected = HashMap::new();
        expected.insert(0, U256::try_from(BigUint::from(100u64)).unwrap());
        expected.insert(1, U256::try_from(BigUint::from(200u64)).unwrap());

        assert_eq!(result, expected);
    }

    #[test]
    fn test_parse_single_fee() {
        let fee_str = "42:123456789";
        let result = parse_fee_str(fee_str).unwrap();

        let mut expected = HashMap::new();
        expected.insert(42, U256::try_from(BigUint::from(123456789u64)).unwrap());

        assert_eq!(result, expected);
    }

    #[test]
    fn test_parse_empty_string() {
        let fee_str = "";
        let result = parse_fee_str(fee_str);

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Parse error: Invalid fee format: should be token_index:fee_amount"
        );
    }

    #[test]
    fn test_invalid_format_missing_colon() {
        let fee_str = "0-100,1:200";
        let result = parse_fee_str(fee_str);

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Parse error: Invalid fee format: should be token_index:fee_amount"
        );
    }

    #[test]
    fn test_hex_token_index() {
        let fee_str = "abc:100";
        let result = parse_fee_str(fee_str);

        assert!(result.is_err());
        assert!(format!("{}", result.err().unwrap()).contains("Failed to parse token index"));
    }

    #[test]
    fn test_hex_fee_amount() {
        let fee_str = "0:abc";
        let result = parse_fee_str(fee_str);

        assert!(result.is_err());
        assert!(format!("{}", result.err().unwrap()).contains("Failed to parse fee amount"));
    }

    #[test]
    fn test_big_fee_amount() {
        let fee_str = "1:1000000000000000000000000000000000000";
        let result = parse_fee_str(fee_str).unwrap();

        let mut expected = HashMap::new();
        expected.insert(
            1u32,
            U256::try_from(
                BigUint::parse_bytes("1000000000000000000000000000000000000".as_bytes(), 10)
                    .unwrap(),
            )
            .unwrap(),
        );

        assert_eq!(result, expected);
    }

    #[test]
    fn test_maximum_u256_fee() {
        // max = 2^256 - 1
        let max = (BigUint::one() << 256) - BigUint::one();
        let fee_str = format!("1:{}", max);
        let result = parse_fee_str(&fee_str).unwrap();

        let mut expected = HashMap::new();
        let u256 = U256::try_from(max).unwrap();
        expected.insert(1, u256);

        assert_eq!(result, expected);
    }

    #[test]
    fn test_fee_amount_overflow_u256() {
        // 2^256 — overflow for U256
        let overflow = BigUint::one() << 256;
        let fee_str = format!("0:{}", overflow);

        let result = parse_fee_str(&fee_str);

        assert!(matches!(
            result,
            Err(FeeError::ParseError(msg)) if msg.contains("Failed to convert fee amount")
        ));
    }

    #[test]
    fn test_negative_fee_amount() {
        let fee_str = "0:-100";
        let result = parse_fee_str(fee_str);

        assert!(matches!(
            result,
            Err(FeeError::ParseError(msg)) if msg.contains("Failed to parse fee amount")
        ));
    }

    #[test]
    fn test_maximum_token_index() {
        let fee_str = format!("{}:42", u32::MAX);
        let result = parse_fee_str(&fee_str).unwrap();

        let mut expected = HashMap::new();
        let big = BigUint::from(42u32);
        let u256 = U256::try_from(big).unwrap();
        expected.insert(u32::MAX, u256);

        assert_eq!(result, expected);
    }
}
