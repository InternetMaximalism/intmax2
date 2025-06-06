use intmax2_zkp::{common::signature_content::key_set::KeySet, ethereum_types::address::Address};

use super::{client::get_client, error::CliError};

pub async fn sync_withdrawals(key: KeySet, fee_token_index: Option<u32>) -> Result<(), CliError> {
    let client = get_client()?;
    let withdrawal_fee = client.withdrawal_server.get_withdrawal_fee().await?;
    let fee_token_index = fee_token_index.unwrap_or(0);
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
    let claim_fee = client.withdrawal_server.get_claim_fee().await?;
    let fee_token_index = fee_token_index.unwrap_or(0);
    client
        .sync_claims(key, recipient, &claim_fee, fee_token_index)
        .await?;
    Ok(())
}

pub async fn resync(key: KeySet, is_deep: bool) -> Result<(), CliError> {
    let client = get_client()?;
    client.resync(key, is_deep).await?;
    Ok(())
}
