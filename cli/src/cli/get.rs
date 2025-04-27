use intmax2_interfaces::data::deposit_data::TokenType;
use intmax2_zkp::common::{signature_content::key_set::KeySet, trees::asset_tree::AssetLeaf};

use crate::cli::{client::get_client, history::format_timestamp};

use super::error::CliError;

pub async fn balance(key: KeySet, sync: bool) -> Result<(), CliError> {
    let client = get_client()?;
    let balances = if sync {
        client.sync(key).await?;
        let user_data = client.get_user_data(key).await?;
        user_data.balances()
    } else {
        client.get_balances_without_sync(key).await?
    };
    let mut balances: Vec<(u32, AssetLeaf)> = balances.0.into_iter().collect();
    balances.sort_by_key(|(i, _leaf)| *i);

    println!("Balances:");
    for (i, leaf) in balances.iter() {
        let (token_type, address, token_id) = client.liquidity_contract.get_token_info(*i).await?;
        println!("\t Token #{}:", i);
        println!("\t\t Amount: {}", leaf.amount);
        println!("\t\t Type: {}", token_type);

        match token_type {
            TokenType::NATIVE => {}
            TokenType::ERC20 => {
                println!("\t\t Address: {}", address);
            }
            TokenType::ERC721 => {
                println!("\t\t Address: {}", address);
                println!("\t\t Token ID: {}", token_id);
            }
            TokenType::ERC1155 => {
                println!("\t\t Address: {}", address);
                println!("\t\t Token ID: {}", token_id);
            }
        }
    }
    Ok(())
}

pub async fn withdrawal_status(key: KeySet) -> Result<(), CliError> {
    let client = get_client()?;
    let withdrawal_info = client.get_withdrawal_info(key).await?;
    println!("Withdrawal status:");
    for (i, withdrawal_info) in withdrawal_info.iter().enumerate() {
        let withdrawal = withdrawal_info.contract_withdrawal.clone();
        println!(
            "#{}: recipient: {}, token_index: {}, amount: {}, status: {}",
            i,
            withdrawal.recipient,
            withdrawal.token_index,
            withdrawal.amount,
            withdrawal_info.status
        );
    }
    Ok(())
}

pub async fn mining_list(key: KeySet) -> Result<(), CliError> {
    let client = get_client()?;
    let minings = client.get_mining_list(key).await?;
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

pub async fn claim_status(key: KeySet) -> Result<(), CliError> {
    let client = get_client()?;
    let claim_info = client.get_claim_info(key).await?;
    println!("Claim status:");
    for (i, claim_info) in claim_info.iter().enumerate() {
        let claim = claim_info.claim.clone();
        println!(
            "#{}: recipient: {}, amount: {}, status: {}",
            i, claim.recipient, claim.amount, claim_info.status
        );
    }
    Ok(())
}
