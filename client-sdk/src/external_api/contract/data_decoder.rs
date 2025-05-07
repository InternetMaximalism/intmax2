use alloy::{primitives::B256, sol_types::SolCall};
use intmax2_zkp::{
    common::{
        block::Block,
        signature_content::{
            block_sign_payload::BlockSignPayload,
            flatten::{FlatG1, FlatG2},
            utils::get_pubkey_hash,
            SignatureContent,
        },
        witness::full_block::FullBlock,
    },
    constants::NUM_SENDERS_IN_BLOCK,
    ethereum_types::{
        account_id::AccountIdPacked, address::Address, bytes32::Bytes32, u256::U256,
        u32limb_trait::U32LimbTrait as _,
    },
};

use crate::external_api::contract::{
    convert::{convert_b128_to_byte16, convert_b256_to_bytes32, convert_u256_to_intmax},
    rollup_contract::Rollup,
};

pub fn decode_post_block_calldata(
    prev_block_hash: Bytes32,
    deposit_tree_root: Bytes32,
    timestamp: u64,
    block_number: u32,
    block_builder_address: Address,
    data: &[u8],
) -> anyhow::Result<FullBlock> {
    let selector: [u8; 4] = data[0..4].try_into().unwrap();
    match selector {
        Rollup::postRegistrationBlockCall::SELECTOR => {
            let decoded = Rollup::postRegistrationBlockCall::abi_decode(data)?;
            let block_sign_payload = BlockSignPayload {
                is_registration_block: true,
                tx_tree_root: convert_b256_to_bytes32(decoded.txTreeRoot),
                expiry: decoded.expiry.into(),
                block_builder_address,
                block_builder_nonce: decoded.builderNonce,
            };
            let pubkeys = decoded
                .senderPublicKeys
                .into_iter()
                .map(convert_u256_to_intmax)
                .collect::<Vec<U256>>();
            let signature = SignatureContent {
                block_sign_payload,
                sender_flag: convert_b128_to_byte16(decoded.senderFlags),
                agg_pubkey: convert_to_flat_g1(decoded.aggregatedPublicKey)?,
                agg_signature: convert_to_flat_g2(decoded.aggregatedSignature)?,
                message_point: convert_to_flat_g2(decoded.messagePoint)?,
                pubkey_hash: pad_pubkey_and_hash(&pubkeys),
                account_id_hash: Bytes32::default(),
            };
            let block = Block {
                prev_block_hash,
                deposit_tree_root,
                signature_hash: signature.hash(),
                timestamp,
                block_number,
            };
            let full_block = FullBlock {
                block,
                signature,
                pubkeys: Some(pubkeys),
                account_ids: None,
            };
            Ok(full_block)
        }
        Rollup::postNonRegistrationBlockCall::SELECTOR => {
            let decoded = Rollup::postNonRegistrationBlockCall::abi_decode(data)?;
            let block_sign_payload = BlockSignPayload {
                is_registration_block: false,
                tx_tree_root: convert_b256_to_bytes32(decoded.txTreeRoot),
                expiry: decoded.expiry.into(),
                block_builder_address,
                block_builder_nonce: decoded.builderNonce,
            };
            let account_id_packed = AccountIdPacked::from_trimmed_bytes(&decoded.senderAccountIds)
                .map_err(|e| anyhow::anyhow!("error while recovering packed account ids {}", e))?;
            let signature = SignatureContent {
                block_sign_payload,
                sender_flag: convert_b128_to_byte16(decoded.senderFlags),
                agg_pubkey: convert_to_flat_g1(decoded.aggregatedPublicKey)?,
                agg_signature: convert_to_flat_g2(decoded.aggregatedSignature)?,
                message_point: convert_to_flat_g2(decoded.messagePoint)?,
                pubkey_hash: convert_b256_to_bytes32(decoded.publicKeysHash),
                account_id_hash: account_id_packed.hash(),
            };
            let block = Block {
                prev_block_hash,
                deposit_tree_root,
                signature_hash: signature.hash(),
                timestamp,
                block_number,
            };
            let full_block = FullBlock {
                block,
                signature,
                pubkeys: None,
                account_ids: Some(decoded.senderAccountIds.to_vec()),
            };
            Ok(full_block)
        }
        _ => {
            anyhow::bail!("Unknown function selector");
        }
    }
}

fn pad_pubkey_and_hash(pubkeys: &[U256]) -> Bytes32 {
    let mut pubkeys = pubkeys.to_vec();
    pubkeys.resize(NUM_SENDERS_IN_BLOCK, U256::dummy_pubkey());
    get_pubkey_hash(&pubkeys)
}

fn convert_to_flat_g1(data: [B256; 2]) -> anyhow::Result<FlatG1> {
    let flat_g1 = FlatG1([
        U256::from_bytes_be(&data[0].0).unwrap(),
        U256::from_bytes_be(&data[1].0).unwrap(),
    ]);
    Ok(flat_g1)
}

fn convert_to_flat_g2(data: [B256; 4]) -> anyhow::Result<FlatG2> {
    let flat_g2 = FlatG2([
        U256::from_bytes_be(&data[0].0).unwrap(),
        U256::from_bytes_be(&data[1].0).unwrap(),
        U256::from_bytes_be(&data[2].0).unwrap(),
        U256::from_bytes_be(&data[3].0).unwrap(),
    ]);
    Ok(flat_g2)
}
