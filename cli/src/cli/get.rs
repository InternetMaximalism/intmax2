use chrono::DateTime;
use colored::{ColoredString, Colorize as _};
use intmax2_client_sdk::client::history::{EntryStatus, HistoryEntry};
use intmax2_interfaces::data::deposit_data::TokenType;
use intmax2_zkp::{
    common::{signature::key_set::KeySet, transfer::Transfer, trees::asset_tree::AssetLeaf},
    ethereum_types::u32limb_trait::U32LimbTrait as _,
};

use crate::cli::client::get_client;

use super::error::CliError;

pub async fn balance(key: KeySet) -> Result<(), CliError> {
    let client = get_client()?;
    client.sync(key).await?;

    let user_data = client.get_user_data(key).await?;
    let mut balances: Vec<(u32, AssetLeaf)> = user_data.balances().0.into_iter().collect();
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

pub async fn claim_status(key: KeySet) -> Result<(), CliError> {
    let client = get_client()?;
    let claim_info = client.get_claim_info(key).await?;
    println!("Withdrawal status:");
    for (i, claim_info) in claim_info.iter().enumerate() {
        let claim = claim_info.claim.clone();
        println!(
            "#{}: recipient: {}, amount: {}, status: {}",
            i, claim.recipient, claim.amount, claim_info.status
        );
    }
    Ok(())
}

pub async fn history(key: KeySet) -> Result<(), CliError> {
    let client = get_client()?;
    let history = client.fetch_history(key).await?;
    println!("History:");
    for entry in history {
        print_history_entry(&entry)?;
        println!();
    }
    Ok(())
}

fn format_timestamp(timestamp: u64) -> String {
    let naive = DateTime::from_timestamp(timestamp as i64, 0).unwrap();
    naive.format("%Y-%m-%d %H:%M:%S UTC").to_string()
}

fn format_status(status: &EntryStatus) -> ColoredString {
    match status {
        EntryStatus::Processed(block_number) => {
            format!("Settled in block {} and processed", block_number).bright_blue()
        }
        EntryStatus::Settled(block_number) => {
            format!("Settled in block {}", block_number).bright_green()
        }
        EntryStatus::Pending => "Pending".bright_yellow(),
        EntryStatus::Timeout => "Timeout".bright_red(),
    }
}

fn format_transfer(transfer: &Transfer) -> String {
    format!(
        "Transfer to {}: token_index: {}, amount: {}",
        if transfer.recipient.is_pubkey {
            transfer.recipient.to_pubkey().unwrap().to_hex()
        } else {
            transfer.recipient.to_address().unwrap().to_hex()
        },
        transfer.token_index,
        transfer.amount
    )
}

fn print_history_entry(entry: &HistoryEntry) -> Result<(), CliError> {
    match entry {
        HistoryEntry::Deposit {
            deposit,
            status,
            meta,
        } => {
            let time = format_timestamp(meta.timestamp);
            println!(
                "{} [{}]",
                "DEPOSIT".bright_green().bold(),
                time.bright_blue(),
            );
            println!("  UUID: {}", meta.uuid);
            println!("  Status: {}", format_status(status));
            println!("  Token: {}", deposit.token_type.to_string().yellow(),);
            println!(
                "      Address: {}",
                deposit.token_address.to_string().cyan()
            );
            println!("      ID: {}", deposit.token_id.to_string().white());
            println!(
                "      Index: {}",
                deposit
                    .token_index
                    .map_or("N/A".to_string(), |idx| idx.to_string())
                    .white()
            );
            println!("  Amount: {}", deposit.amount.to_string().bright_green());
            println!(
                "  Deposit Hash: {}",
                deposit
                    .deposit_hash()
                    .map_or("N/A".to_string(), |h| h.to_string())
            );
        }
        HistoryEntry::Receive {
            transfer,
            status,
            meta,
        } => {
            let time = format_timestamp(meta.timestamp);

            println!(
                "{} [{}]",
                "RECEIVE".bright_purple().bold(),
                time.bright_blue(),
            );
            println!("  UUID: {}", meta.uuid);
            println!("  Status: {}", format_status(status));
            println!("  From: {}", transfer.sender.to_hex().yellow());
            println!(
                "  Token Index: {}",
                transfer.transfer.token_index.to_string().white()
            );
            println!(
                "  Amount: {}",
                transfer.transfer.amount.to_string().bright_green()
            );
        }
        HistoryEntry::Send { tx, status, meta } => {
            let time = format_timestamp(meta.timestamp);
            println!("{} [{}]", "SEND".bright_red().bold(), time.bright_blue(),);
            println!("  UUID: {}", meta.uuid);
            println!("  Status: {}", format_status(status));
            println!("  Transfers:");
            for (i, transfer) in tx.spent_witness.transfers.iter().enumerate() {
                if transfer == &Transfer::default() {
                    // ignore dummy transfers
                    continue;
                }
                println!("    {}: {}", i, format_transfer(transfer).white());
            }
        }
    }
    Ok(())
}
