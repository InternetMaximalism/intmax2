use intmax2_interfaces::utils::key::ViewPair;
use intmax2_zkp::ethereum_types::address::Address;

use super::{client::get_client, error::CliError};

pub async fn sync_withdrawals(
    view_pair: ViewPair,
    fee_token_index: Option<u32>,
) -> Result<(), CliError> {
    let client = get_client()?;
    let withdrawal_fee = client.withdrawal_server.get_withdrawal_fee().await?;
    let fee_token_index = fee_token_index.unwrap_or(0);
    client
        .sync_withdrawals(view_pair, &withdrawal_fee, fee_token_index)
        .await?;
    Ok(())
}

pub async fn sync_claims(
    view_pair: ViewPair,
    recipient: Address,
    fee_token_index: Option<u32>,
) -> Result<(), CliError> {
    let client = get_client()?;
    let claim_fee = client.withdrawal_server.get_claim_fee().await?;
    let fee_token_index = fee_token_index.unwrap_or(0);
    client
        .sync_claims(view_pair, recipient, &claim_fee, fee_token_index)
        .await?;
    Ok(())
}

pub async fn resync(view_pair: ViewPair, is_deep: bool) -> Result<(), CliError> {
    let client = get_client()?;
    client.resync(view_pair, is_deep).await?;
    Ok(())
}
