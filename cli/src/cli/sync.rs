use ethers::types::Address;
use intmax2_interfaces::api::withdrawal_server::interface::WithdrawalServerClientInterface;
use intmax2_zkp::common::signature::key_set::KeySet;

use super::{client::get_client, error::CliError, utils::convert_address};

pub async fn sync_withdrawals(key: KeySet) -> Result<(), CliError> {
    let client = get_client()?;
    let withdrawal_fee = client.withdrawal_server.get_withdrawal_fee().await?;
    client.sync_withdrawals(key, withdrawal_fee).await?;
    Ok(())
}

pub async fn sync_claims(key: KeySet, recipient: Address) -> Result<(), CliError> {
    let client = get_client()?;
    let recipient = convert_address(recipient);
    client.sync_claims(key, recipient).await?;
    Ok(())
}
