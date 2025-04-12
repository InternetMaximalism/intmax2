use ethers::abi::{Functions, Token};
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
        account_id::AccountIdPacked, address::Address, bytes16::Bytes16, bytes32::Bytes32,
        u256::U256, u32limb_trait::U32LimbTrait as _, u64::U64,
    },
};

pub fn decode_post_block_calldata(
    functions: Functions,
    prev_block_hash: Bytes32,
    deposit_tree_root: Bytes32,
    timestamp: u64,
    block_number: u32,
    block_builder_address: Address,
    data: &[u8],
) -> anyhow::Result<FullBlock> {
    let signature = &data[0..4];
    let function = functions
        .into_iter()
        .find(|f| &f.short_signature()[..4] == signature)
        .ok_or(anyhow::anyhow!("Function not found"))?;
    let decoded = function.decode_input(&data[4..]).map_err(|e| {
        anyhow::anyhow!(
            "Failed to decode input data for function {} with error: {}",
            function.name,
            e
        )
    })?;

    let full_block = match function.name.as_str() {
        "postRegistrationBlock" => parse_block(
            true,
            prev_block_hash,
            deposit_tree_root,
            timestamp,
            block_number,
            block_builder_address,
            &decoded,
        )?,
        "postNonRegistrationBlock" => parse_block(
            false,
            prev_block_hash,
            deposit_tree_root,
            timestamp,
            block_number,
            block_builder_address,
            &decoded,
        )?,
        _ => {
            anyhow::bail!("Function not supported");
        }
    };
    Ok(full_block)
}

fn parse_block(
    is_registration_block: bool,
    prev_block_hash: Bytes32,
    deposit_tree_root: Bytes32,
    timestamp: u64,
    block_number: u32,
    block_builder_address: Address,
    decoded: &[Token],
) -> anyhow::Result<FullBlock> {
    let tx_tree_root = decoded
        .first()
        .ok_or(anyhow::anyhow!("tx_tree_root not found"))?
        .clone()
        .into_fixed_bytes()
        .ok_or(anyhow::anyhow!("tx_tree_root is not FixedBytes"))?;
    let tx_tree_root = Bytes32::from_bytes_be(&tx_tree_root).unwrap();
    let expiry = decoded
        .get(1)
        .ok_or(anyhow::anyhow!("expiry not found"))?
        .clone()
        .into_uint()
        .ok_or(anyhow::anyhow!("expiry is not Uint"))?;
    let expiry: U64 = expiry.as_u64().into();
    let block_builder_nonce = decoded
        .get(2)
        .ok_or(anyhow::anyhow!("builder_nonce not found"))?
        .clone()
        .into_uint()
        .ok_or(anyhow::anyhow!("builder_nonce is not Uint"))?;
    let block_builder_nonce = block_builder_nonce.as_u32();
    let sender_flag = decoded
        .get(3)
        .ok_or(anyhow::anyhow!("sender_flags not found"))?
        .clone()
        .into_fixed_bytes()
        .ok_or(anyhow::anyhow!("sender_flags is not FixedBytes"))?;
    let sender_flag = Bytes16::from_bytes_be(&sender_flag).unwrap();
    let aggregated_public_key = decoded
        .get(4)
        .ok_or(anyhow::anyhow!("aggregated_public_key not found"))?
        .clone()
        .into_fixed_array()
        .ok_or(anyhow::anyhow!("aggregated_public_key is not FixedArray"))?
        .iter()
        .map(|token| {
            token.clone().into_fixed_bytes().ok_or(anyhow::anyhow!(
                "aggregated_public_key element is not FixedBytes"
            ))
        })
        .collect::<anyhow::Result<Vec<Vec<u8>>>>()?;
    let agg_pubkey = FlatG1(
        aggregated_public_key
            .iter()
            .map(|e| U256::from_bytes_be(e).unwrap())
            .collect::<Vec<U256>>()
            .try_into()
            .map_err(|_| anyhow::anyhow!("aggregated_public_key is not FlatG1"))?,
    );
    let aggregated_signature = decoded
        .get(5)
        .ok_or(anyhow::anyhow!("aggregated_signature not found"))?
        .clone()
        .into_fixed_array()
        .ok_or(anyhow::anyhow!("aggregated_signature is not FixedArray"))?
        .iter()
        .map(|token| {
            token.clone().into_fixed_bytes().ok_or(anyhow::anyhow!(
                "aggregated_signature element is not FixedBytes"
            ))
        })
        .collect::<anyhow::Result<Vec<Vec<u8>>>>()?;
    let agg_signature = FlatG2(
        aggregated_signature
            .iter()
            .map(|e| U256::from_bytes_be(e).unwrap())
            .collect::<Vec<U256>>()
            .try_into()
            .map_err(|_| anyhow::anyhow!("aggregated_signature is not FlatG2"))?,
    );
    let message_point = decoded
        .get(6)
        .ok_or(anyhow::anyhow!("message_point not found"))?
        .clone()
        .into_fixed_array()
        .ok_or(anyhow::anyhow!("message_point is not FixedArray"))?
        .iter()
        .map(|token| {
            token
                .clone()
                .into_fixed_bytes()
                .ok_or(anyhow::anyhow!("message_point element is not FixedBytes"))
        })
        .collect::<anyhow::Result<Vec<Vec<u8>>>>()?;
    let message_point = FlatG2(
        message_point
            .iter()
            .map(|e| U256::from_bytes_be(e).unwrap())
            .collect::<Vec<U256>>()
            .try_into()
            .map_err(|_| anyhow::anyhow!("message_point is not FlatG2"))?,
    );

    let pubkeys = if is_registration_block {
        let pubkeys = decoded.get(7).ok_or(anyhow::anyhow!("pubkeys not found"))?;
        Some(parse_sender_public_keys(pubkeys.clone())?)
    } else {
        None
    };
    let account_ids = if is_registration_block {
        None
    } else {
        let account_ids = decoded
            .get(8) // note that index=5 is pubkeys_hash
            .ok_or(anyhow::anyhow!("account_ids not found"))?;
        Some(parse_account_ids(account_ids.clone())?)
    };

    let pubkey_hash = if is_registration_block {
        let mut pubkeys = pubkeys.as_ref().unwrap().clone();
        pubkeys.resize(NUM_SENDERS_IN_BLOCK, U256::dummy_pubkey());
        get_pubkey_hash(&pubkeys)
    } else {
        let pubkey_hash = decoded
            .get(7)
            .ok_or(anyhow::anyhow!("pubkey_hash is not found"))?
            .clone()
            .into_fixed_bytes()
            .ok_or(anyhow::anyhow!("pubkey_hash is not FixedBytes"))?;
        Bytes32::from_bytes_be(&pubkey_hash).unwrap()
    };
    let account_id_hash = if is_registration_block {
        Bytes32::default()
    } else {
        let account_ids_packed = AccountIdPacked::from_trimmed_bytes(account_ids.as_ref().unwrap())
            .map_err(|e| anyhow::anyhow!("error while recovering packed account ids {}", e))?;
        account_ids_packed.hash()
    };
    let block_sign_payload = BlockSignPayload {
        is_registration_block,
        tx_tree_root,
        expiry,
        block_builder_address,
        block_builder_nonce,
    };

    let signature = SignatureContent {
        block_sign_payload,
        sender_flag,
        agg_pubkey,
        agg_signature,
        message_point,
        pubkey_hash,
        account_id_hash,
    };

    let block = Block {
        prev_block_hash,
        deposit_tree_root,
        signature_hash: signature.hash(),
        timestamp,
        block_number,
    };

    Ok(FullBlock {
        block,
        signature,
        pubkeys,
        account_ids,
    })
}

fn parse_sender_public_keys(decoded: Token) -> anyhow::Result<Vec<U256>> {
    let sender_public_keys = decoded
        .into_array()
        .ok_or(anyhow::anyhow!("sender_public_keys is not Array"))?
        .iter()
        .map(|token| {
            token.clone().into_uint().ok_or(anyhow::anyhow!(
                "sender_public_keys element is not FixedBytes"
            ))
        })
        .collect::<anyhow::Result<Vec<ethers::types::U256>>>()?;
    let sender_public_keys = sender_public_keys
        .into_iter()
        .map(|e| {
            let mut bytes = [0u8; 32];
            e.to_big_endian(&mut bytes);
            U256::from_bytes_be(&bytes).unwrap()
        })
        .collect::<Vec<U256>>();
    Ok(sender_public_keys)
}

// account_ids: Vec<u8>,
fn parse_account_ids(decoded: Token) -> anyhow::Result<Vec<u8>> {
    let account_ids = decoded
        .into_bytes()
        .ok_or(anyhow::anyhow!("account_ids is not Bytes"))?;
    Ok(account_ids)
}
