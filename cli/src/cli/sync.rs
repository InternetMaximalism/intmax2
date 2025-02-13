use ethers::types::Address;
use intmax2_interfaces::api::withdrawal_server::interface::WithdrawalServerClientInterface;
use intmax2_zkp::common::signature::key_set::KeySet;

use super::{client::get_client, error::CliError, utils::convert_address};

pub async fn sync_withdrawals(key: KeySet, fee_token_index: Option<u32>) -> Result<(), CliError> {
    let client = get_client()?;
    let withdrawal_fee = client.withdrawal_server.get_withdrawal_fee().await?;
    client
        .sync_withdrawals(key, &withdrawal_fee, fee_token_index)
        .await?;
    Ok(())
}

pub async fn sync_claims(
    key: KeySet,
    recipient: Address,
    fee_token_index: Option<u32>,
) -> Result<(), CliError> {
    let client = get_client()?;
    let recipient = convert_address(recipient);
    let claim_fee = client.withdrawal_server.get_claim_fee().await?;
    client
        .sync_claims(key, recipient, &claim_fee, fee_token_index)
        .await?;
    Ok(())
}
