use chrono::DateTime;
use colored::{ColoredString, Colorize as _};
use intmax2_client_sdk::client::history::EntryStatus;
use intmax2_interfaces::{
    api::store_vault_server::types::{CursorOrder, MetaDataCursor},
    data::{
        deposit_data::DepositData, meta_data::MetaData, transfer_data::TransferData,
        tx_data::TxData,
    },
};
use intmax2_zkp::{
    common::{signature_content::key_set::KeySet, transfer::Transfer},
    ethereum_types::{bytes32::Bytes32, u32limb_trait::U32LimbTrait as _},
};

use crate::cli::client::get_client;

use super::error::CliError;

pub async fn history(
    key: KeySet,
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
    let (deposit_history, _) = client.fetch_deposit_history(key, &cursor).await?;
    let (transfer_history, _) = client.fetch_transfer_history(key, &cursor).await?;
    let (tx_history, _) = client.fetch_tx_history(key, &cursor).await?;

    let mut history: Vec<HistoryEum> = Vec::new();
    for entry in deposit_history {
        history.push(HistoryEum::Deposit {
            deposit: entry.data,
            status: entry.status,
            meta: entry.meta,
        });
    }
    for entry in transfer_history {
        history.push(HistoryEum::Receive {
            transfer: entry.data,
            status: entry.status,
            meta: entry.meta,
        });
    }
    for entry in tx_history {
        history.push(HistoryEum::Send {
            tx: entry.data,
            status: entry.status,
            meta: entry.meta,
        });
    }

    history.sort_by_key(|entry| match entry {
        HistoryEum::Deposit { meta, .. } => (meta.timestamp, meta.digest.to_hex()),
        HistoryEum::Receive { meta, .. } => (meta.timestamp, meta.digest.to_hex()),
        HistoryEum::Send { meta, .. } => (meta.timestamp, meta.digest.to_hex()),
    });
    if order == CursorOrder::Desc {
        history.reverse();
    }

    println!("History:");
    for entry in history {
        print_history_entry(&entry)?;
        println!();
    }
    Ok(())
}

pub fn format_timestamp(timestamp: u64) -> String {
    let naive = DateTime::from_timestamp(timestamp as i64, 0).unwrap();
    naive.format("%Y-%m-%d %H:%M:%S UTC").to_string()
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

enum HistoryEum {
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

fn print_history_entry(entry: &HistoryEum) -> Result<(), CliError> {
    match entry {
        HistoryEum::Deposit {
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
            println!("  Digest: {}", meta.digest);
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
        HistoryEum::Receive {
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
            println!("  Digest: {}", meta.digest);
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
        HistoryEum::Send { tx, status, meta } => {
            let time = format_timestamp(meta.timestamp);
            println!("{} [{}]", "SEND".bright_red().bold(), time.bright_blue(),);
            println!("  Digest: {}", meta.digest);
            println!("  Status: {}", format_status(status));
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
    }
    Ok(())
}
