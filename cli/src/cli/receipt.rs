use colored::Colorize as _;
use intmax2_interfaces::utils::{address::IntmaxAddress, key::ViewPair};
use intmax2_zkp::ethereum_types::bytes32::Bytes32;

use crate::cli::{client::get_client, error::CliError};

pub async fn generate_receipt(
    view_pair: ViewPair,
    tx_digest: Bytes32,
    transfer_index: u32,
) -> Result<(), CliError> {
    let client = get_client()?;
    let receipt = client
        .generate_transfer_receipt(view_pair, tx_digest, transfer_index)
        .await?;
    println!("Generated transfer receipt:");
    println!("{}", receipt.yellow());
    Ok(())
}

pub async fn verify_receipt(view_pair: ViewPair, receipt: &str) -> Result<(), CliError> {
    let client = get_client()?;
    let transfer_data = client.validate_transfer_receipt(view_pair, receipt).await?;
    println!("Verified transfer receipt:");
    let sender = IntmaxAddress::from_public_keypair(client.config.network, &transfer_data.sender);
    let recipient = IntmaxAddress::from_viewpair(client.config.network, &view_pair);
    println!("  From: {}", sender.to_string().yellow());
    println!("  To: {}", recipient.to_string().yellow());
    println!(
        "  Token Index: {}",
        transfer_data.transfer.token_index.to_string().white()
    );
    println!(
        "  Amount: {}",
        transfer_data.transfer.amount.to_string().bright_green()
    );
    Ok(())
}
