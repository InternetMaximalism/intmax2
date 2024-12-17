use anyhow::{bail, ensure};
use intmax2_client_sdk::external_api::indexer::IndexerClient;
use intmax2_interfaces::api::indexer::interface::IndexerClientInterface;
use intmax2_zkp::{
    common::{
        generic_address::GenericAddress, salt::Salt, signature::key_set::KeySet, transfer::Transfer,
    },
    constants::NUM_TRANSFERS_IN_TX,
    ethereum_types::{
        address::Address as IAddress, u256::U256 as IU256, u32limb_trait::U32LimbTrait,
    },
};
use serde::Deserialize;

use crate::{
    cli::{client::get_client, utils::convert_u256},
    env_var::EnvVar,
};

use super::error::CliError;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferInput {
    pub recipient: String,
    pub amount: u128,
    pub token_index: u32,
}

pub async fn transfer(key: KeySet, transfer_inputs: &[TransferInput]) -> Result<(), CliError> {
    let mut rng = rand::thread_rng();
    if transfer_inputs.len() > NUM_TRANSFERS_IN_TX {
        return Err(CliError::TooManyTransfer(transfer_inputs.len()));
    }

    let transfers = transfer_inputs
        .iter()
        .map(|input| {
            let recipient = parse_generic_address(&input.recipient)
                .map_err(|e| CliError::ParseError(format!("Failed to parse recipient: {}", e)))?;
            let amount = convert_u256(input.amount.into());
            let token_index = input.token_index;
            let salt = Salt::rand(&mut rng);
            Ok(Transfer {
                recipient,
                amount,
                token_index,
                salt,
            })
        })
        .collect::<Result<Vec<_>, CliError>>()?;

    let env = envy::from_env::<EnvVar>()?;
    let client = get_client()?;

    let pending_info = client.sync(key.clone()).await?;
    log::info!(
        "Pending deposits: {:?}",
        pending_info.pending_deposits.len()
    );
    log::info!(
        "Pending transfers: {:?}",
        pending_info.pending_transfers.len()
    );

    // override block builder base url if it is set in the env
    let block_builder_url = if let Some(block_builder_base_url) = env.block_builder_base_url {
        block_builder_base_url.to_string()
    } else {
        // get block builder info
        let indexer = IndexerClient::new(&env.indexer_base_url.to_string());
        let block_builder_info = indexer.get_block_builder_info().await?;
        if block_builder_info.is_empty() {
            return Err(CliError::UnexpectedError(
                "Block builder info is empty".to_string(),
            ));
        }
        block_builder_info.first().unwrap().url.clone()
    };

    let memo = client
        .send_tx_request(&block_builder_url, key, transfers)
        .await?;

    let is_registration_block = memo.is_registration_block;
    let tx = memo.tx.clone();

    log::info!("Waiting for block builder to build the block");
    tokio::time::sleep(std::time::Duration::from_secs(
        env.block_builder_query_wait_time,
    ))
    .await;

    let mut tries = 0;
    let proposal = loop {
        let proposal = client
            .query_proposal(&block_builder_url, key, is_registration_block, tx)
            .await?;
        if proposal.is_some() {
            break proposal.unwrap();
        }
        if tries > env.block_builder_query_limit {
            return Err(CliError::FailedToGetProposal);
        }
        tries += 1;
        log::info!(
            "Failed to get proposal, retrying in {} seconds",
            env.block_builder_query_interval
        );
        tokio::time::sleep(std::time::Duration::from_secs(
            env.block_builder_query_interval,
        ))
        .await;
    };

    log::info!("Finalizing tx");
    client
        .finalize_tx(&block_builder_url, key, &memo, &proposal)
        .await?;

    Ok(())
}

fn parse_generic_address(address: &str) -> anyhow::Result<GenericAddress> {
    ensure!(address.starts_with("0x"), "Invalid prefix");
    let bytes = hex::decode(&address[2..])?;
    if bytes.len() == 20 {
        let address = IAddress::from_bytes_be(&bytes);
        return Ok(GenericAddress::from_address(address));
    } else if bytes.len() == 32 {
        let pubkey = IU256::from_bytes_be(&bytes);
        return Ok(GenericAddress::from_pubkey(pubkey));
    } else {
        bail!("Invalid length");
    }
}
