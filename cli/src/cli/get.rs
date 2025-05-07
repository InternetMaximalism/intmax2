use intmax2_interfaces::data::deposit_data::TokenType;
use intmax2_zkp::{
    common::{signature_content::key_set::KeySet, trees::asset_tree::AssetLeaf},
    ethereum_types::{address::Address, u256::U256, u32limb_trait::U32LimbTrait},
};

use crate::cli::{client::get_client, history::format_timestamp};

use super::error::CliError;

pub struct BalanceInfo {
    pub token_index: u32,
    pub amount: U256,
    pub token_type: TokenType,
    pub address: Option<Address>,
    pub token_id: Option<U256>,
}

pub async fn balance(key: KeySet, sync: bool) -> Result<Vec<BalanceInfo>, CliError> {
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

    let mut total_balance = vec![];
    for (i, leaf) in balances.iter() {
        let (token_type, address, token_id) = client.liquidity_contract.get_token_info(*i).await?;
        let (address, token_id) = match token_type {
            TokenType::NATIVE => (None, None),
            TokenType::ERC20 => (Some(address), None),
            TokenType::ERC721 => (Some(address), Some(token_id)),
            TokenType::ERC1155 => (Some(address), Some(token_id)),
        };
        total_balance.push(BalanceInfo {
            token_index: *i,
            amount: leaf.amount,
            token_type,
            address,
            token_id,
        });
    }

    Ok(total_balance)
}

pub fn log_balance(total_balance: Vec<BalanceInfo>) {
    println!("Balances:");
    for balance in total_balance {
        println!("Token #{}:", balance.token_index);
        println!("\tAmount: {}", balance.amount);
        println!("\tType: {}", balance.token_type);
        if let Some(address) = balance.address {
            println!("\tAddress: {}", address);
        }
        if let Some(token_id) = balance.token_id {
            println!("\tToken ID: {}", token_id);
        }
    }
}

pub async fn withdrawal_status(key: KeySet) -> Result<(), CliError> {
    let client = get_client()?;
    let withdrawal_info = client.get_withdrawal_info(key).await?;
    println!("Withdrawal status:");
    for (i, withdrawal_info) in withdrawal_info.iter().enumerate() {
        let withdrawal = withdrawal_info.contract_withdrawal.clone();
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
        let l1_tx_hash = claim_info
            .l1_tx_hash
            .map_or("N/A".to_string(), |h| h.to_hex());
        println!(
            "#{}: recipient: {}, amount: {}, l1_tx_hash: {}, status: {}",
            i, claim.recipient, claim.amount, l1_tx_hash, claim_info.status
        );
    }
    Ok(())
}

pub async fn check_validity_prover() -> Result<(), CliError> {
    let client = get_client()?;
    client.check_validity_prover().await?;
    Ok(())
}
