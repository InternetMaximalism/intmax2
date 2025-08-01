use colored::Colorize;
use futures::future::try_join_all;
use intmax2_client_sdk::client::{
    misc::payment_memo::get_all_payment_memos,
    strategy::strategy::{fetch_all_claim_infos, fetch_all_withdrawal_infos},
};
use intmax2_interfaces::{data::deposit_data::TokenType, utils::key::ViewPair};
use intmax2_zkp::{
    common::trees::asset_tree::AssetLeaf, ethereum_types::u32limb_trait::U32LimbTrait,
};

use crate::cli::{client::get_client, history::format_timestamp};

use super::error::CliError;

pub async fn balance(view_pair: ViewPair, sync: bool) -> Result<(), CliError> {
    let client = get_client()?;
    let balances = if sync {
        client.sync(view_pair).await?;
        let user_data = client.get_user_data(view_pair).await?;
        user_data.balances()
    } else {
        client.get_balances_without_sync(view_pair).await?
    };
    let mut balances: Vec<(u32, AssetLeaf)> = balances.0.into_iter().collect();
    balances.sort_by_key(|(i, _leaf)| *i);

    let token_info_futures = balances
        .iter()
        .map(|(i, _)| client.liquidity_contract.get_token_info(*i));
    let token_infos = try_join_all(token_info_futures).await?;

    println!("Balances:");
    for ((i, leaf), (token_type, address, token_id)) in balances.iter().zip(token_infos) {
        //let (token_type, address, token_id) = client.liquidity_contract.get_token_info(*i).await?;
        println!("\t Token #{i}:");
        println!("\t\t Amount: {}", leaf.amount);
        println!("\t\t Type: {token_type}");

        match token_type {
            TokenType::NATIVE => {}
            TokenType::ERC20 => {
                println!("\t\t Address: {address}");
            }
            TokenType::ERC721 => {
                println!("\t\t Address: {address}");
                println!("\t\t Token ID: {token_id}");
            }
            TokenType::ERC1155 => {
                println!("\t\t Address: {address}");
                println!("\t\t Token ID: {token_id}");
            }
        }
    }
    Ok(())
}

pub async fn withdrawal_status(view_pair: ViewPair) -> Result<(), CliError> {
    let client = get_client()?;
    let withdrawal_info =
        fetch_all_withdrawal_infos(client.withdrawal_server.as_ref(), view_pair).await?;
    println!("Withdrawal status:");
    for (i, withdrawal_info) in withdrawal_info.iter().enumerate() {
        let withdrawal = &withdrawal_info.contract_withdrawal;
        let l1_tx_hash = withdrawal_info
            .l1_tx_hash
            .map_or("N/A".to_string(), |h| h.to_hex());
        println!(
            "#{}: recipient: {}, token_index: {}, amount: {}, l1_tx_hash: {}, status: {}",
            i,
            withdrawal.recipient,
            withdrawal.token_index,
            withdrawal.amount,
            l1_tx_hash,
            withdrawal_info.status
        );
    }
    Ok(())
}

pub async fn mining_list(view_pair: ViewPair) -> Result<(), CliError> {
    let client = get_client()?;
    let minings = client.get_mining_list(view_pair).await?;
    println!("Mining list:");
    for (i, mining) in minings.iter().enumerate() {
        let block_number = mining
            .block
            .as_ref()
            .map_or("N/A".to_string(), |b| b.block_number.to_string());
        let maturity = mining.maturity.map_or("N/A".to_string(), format_timestamp);
        println!(
            "#{}: deposit included block :{}, deposit amount: {}, maturity: {}, status: {}",
            i, block_number, mining.deposit_data.amount, maturity, mining.status
        );
    }
    Ok(())
}

pub async fn claim_status(view_pair: ViewPair) -> Result<(), CliError> {
    let client = get_client()?;
    let claim_info = fetch_all_claim_infos(client.withdrawal_server.as_ref(), view_pair).await?;
    println!("Claim status:");
    for (i, claim_info) in claim_info.iter().enumerate() {
        let claim = claim_info.claim.clone();
        let submit_proof_tx_hash = claim_info
            .submit_claim_proof_tx_hash
            .map_or("N/A".to_string(), |h| h.to_hex());
        let l1_tx_hash = claim_info
            .l1_tx_hash
            .map_or("N/A".to_string(), |h| h.to_hex());
        println!(
            "#{}: recipient: {}, amount: {}, submit_proof_tx_hash: {}, l1_tx_hash: {}, status: {}",
            i, claim.recipient, claim.amount, submit_proof_tx_hash, l1_tx_hash, claim_info.status
        );
    }
    Ok(())
}

pub async fn check_validity_prover() -> Result<(), CliError> {
    let client = get_client()?;
    client.check_validity_prover().await?;
    Ok(())
}

pub async fn get_payment_memos(view_pair: ViewPair, name: &str) -> Result<(), CliError> {
    let client = get_client()?;
    let payment_memos =
        get_all_payment_memos(client.store_vault_server.as_ref(), view_pair.view, name).await?;
    println!("Payment memos:");
    for (i, memo) in payment_memos.iter().enumerate() {
        println!(
            "#{}: digest: {}, timestamp: {}, memo: {}",
            i,
            memo.meta.digest.to_hex(),
            format_timestamp(memo.meta.timestamp),
            memo.memo
        );
    }
    Ok(())
}

pub async fn get_user_data(view_pair: ViewPair) -> Result<(), CliError> {
    let client = get_client()?;
    let user_data = client.get_user_data(view_pair).await?;
    println!(
        "{}: {:?}\n",
        "Nullifiers".bright_magenta(),
        user_data.full_private_state.nullifier_tree.nullifiers()
    );
    println!(
        "{}: {:?}\n",
        "Deposit Status".bright_blue(),
        user_data.deposit_status
    );
    println!(
        "{}: {:?}\n",
        "Transfer Status".bright_green(),
        user_data.transfer_status
    );
    println!("{}: {:?}\n", "Tx Status".bright_cyan(), user_data.tx_status);
    println!(
        "{}: {:?}\n",
        "Withdrawal Status".bright_yellow(),
        user_data.withdrawal_status
    );
    println!(
        "{}: {:?}\n",
        "Claim Status".bright_red(),
        user_data.claim_status
    );
    Ok(())
}
