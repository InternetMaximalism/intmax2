use chrono::DateTime;
use colored::{ColoredString, Colorize as _};
use intmax2_client_sdk::client::history::EntryStatus;
use intmax2_interfaces::{
    api::store_vault_server::types::{CursorOrder, MetaDataCursor},
    data::{
        deposit_data::DepositData, meta_data::MetaData, transfer_data::TransferData,
        tx_data::TxData,
    },
    utils::key::ViewPair,
};
use intmax2_zkp::{
    common::transfer::Transfer,
    ethereum_types::{bytes32::Bytes32, u32limb_trait::U32LimbTrait as _},
};

use crate::cli::client::get_client;

use super::error::CliError;

enum HistoryEntry {
    Deposit {
        deposit: DepositData,
        status: EntryStatus,
        meta: MetaData,
    },
    Receive {
        transfer: TransferData,
        status: EntryStatus,
        meta: MetaData,
    },
    Send {
        tx: TxData,
        status: EntryStatus,
        meta: MetaData,
    },
}

pub async fn history(
    view_pair: ViewPair,
    order: CursorOrder,
    from_timestamp: Option<u64>,
) -> Result<(), CliError> {
    let cursor = MetaDataCursor {
        cursor: from_timestamp.map(|timestamp| MetaData {
            timestamp,
            digest: Bytes32::default(),
        }),
        order: order.clone(),
        limit: None,
    };

    let client = get_client()?;
    let (deposit_history, _) = client.fetch_deposit_history(view_pair, &cursor).await?;
    let (transfer_history, _) = client.fetch_transfer_history(view_pair, &cursor).await?;
    let (tx_history, _) = client.fetch_tx_history(view_pair, &cursor).await?;

    let mut history: Vec<HistoryEntry> = deposit_history
        .into_iter()
        .map(|entry| HistoryEntry::Deposit {
            deposit: entry.data,
            status: entry.status,
            meta: entry.meta,
        })
        .chain(
            transfer_history
                .into_iter()
                .map(|entry| HistoryEntry::Receive {
                    transfer: entry.data,
                    status: entry.status,
                    meta: entry.meta,
                }),
        )
        .chain(tx_history.into_iter().map(|entry| HistoryEntry::Send {
            tx: entry.data,
            status: entry.status,
            meta: entry.meta,
        }))
        .collect();

    history.sort_by(|a, b| {
        let a_key = match a {
            HistoryEntry::Deposit { meta, .. }
            | HistoryEntry::Receive { meta, .. }
            | HistoryEntry::Send { meta, .. } => (meta.timestamp, meta.digest.to_hex()),
        };
        let b_key = match b {
            HistoryEntry::Deposit { meta, .. }
            | HistoryEntry::Receive { meta, .. }
            | HistoryEntry::Send { meta, .. } => (meta.timestamp, meta.digest.to_hex()),
        };
        match order {
            CursorOrder::Asc => a_key.cmp(&b_key),
            CursorOrder::Desc => b_key.cmp(&a_key),
        }
    });

    println!("History:");
    for entry in history {
        print_history_entry(&entry)?;
        println!();
    }
    Ok(())
}

pub fn format_timestamp(timestamp: u64) -> String {
    match DateTime::from_timestamp(timestamp as i64, 0) {
        Some(dt) => dt.format("%Y-%m-%d %H:%M:%S UTC").to_string(),
        None => format!("Invalid timestamp: {timestamp}"),
    }
}

fn format_status(status: &EntryStatus) -> ColoredString {
    match status {
        EntryStatus::Processed(block_number) => {
            format!("Settled in block {block_number} and processed").bright_blue()
        }
        EntryStatus::Settled(block_number) => {
            format!("Settled in block {block_number}").bright_green()
        }
        EntryStatus::Pending => "Pending".bright_yellow(),
        EntryStatus::Timeout => "Timeout".bright_red(),
    }
}

fn format_transfer(transfer: &Transfer, transfer_type: &str, transfer_digest: Bytes32) -> String {
    format!(
        "Transfer ({}) to {}: token_index: {}, amount: {}, digest: {}",
        transfer_type,
        if transfer.recipient.is_pubkey {
            transfer.recipient.to_pubkey().unwrap().to_hex()
        } else {
            transfer.recipient.to_address().unwrap().to_hex()
        },
        transfer.token_index,
        transfer.amount,
        transfer_digest.to_hex()
    )
}

fn print_history_entry(entry: &HistoryEntry) -> Result<(), CliError> {
    match entry {
        HistoryEntry::Deposit {
            deposit,
            status,
            meta,
        } => print_deposit_entry(deposit, status, meta),
        HistoryEntry::Receive {
            transfer,
            status,
            meta,
        } => print_receive_entry(transfer, status, meta),
        HistoryEntry::Send { tx, status, meta } => print_send_entry(tx, status, meta),
    }
    Ok(())
}

fn print_digest_status(meta: &MetaData, status: &EntryStatus, label: &str) {
    let time = format_timestamp(meta.timestamp);
    println!("{} [{}]", label, time.bright_blue());
    println!("  Digest: {}", meta.digest);
    println!("  Status: {}", format_status(status));
}

fn print_deposit_entry(deposit: &DepositData, status: &EntryStatus, meta: &MetaData) {
    print_digest_status(meta, status, &"DEPOSIT".bright_green().bold());

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

fn print_receive_entry(transfer: &TransferData, status: &EntryStatus, meta: &MetaData) {
    print_digest_status(meta, status, &"RECEIVE".bright_purple().bold());

    println!("  From: {}", transfer.sender.to_string().yellow());
    println!(
        "  Token Index: {}",
        transfer.transfer.token_index.to_string().white()
    );
    println!(
        "  Amount: {}",
        transfer.transfer.amount.to_string().bright_green()
    );
}

fn print_send_entry(tx: &TxData, status: &EntryStatus, meta: &MetaData) {
    print_digest_status(meta, status, &"SEND".bright_red().bold());

    println!(
        "  Tx Nonce: {}",
        tx.spent_witness.tx.nonce.to_string().cyan()
    );
    println!("  Transfers:");
    let transfers = tx
        .spent_witness
        .transfers
        .iter()
        .filter(|transfer| transfer != &&Transfer::default())
        .collect::<Vec<_>>();

    for (i, ((transfer, transfer_type), transfer_digest)) in transfers
        .iter()
        .zip(tx.transfer_types.iter())
        .zip(tx.transfer_digests.iter())
        .enumerate()
    {
        println!(
            "    {}: {}",
            i,
            format_transfer(transfer, transfer_type, *transfer_digest).white()
        );
    }
}
