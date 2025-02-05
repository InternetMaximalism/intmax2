use ethers::types::Address;
use intmax2_zkp::common::signature::key_set::KeySet;

use super::{client::get_client, error::CliError, utils::convert_address};

pub async fn sync_withdrawals(key: KeySet) -> Result<(), CliError> {
    let client = get_client()?;
    client.sync_withdrawals(key).await?;
    Ok(())
}

pub async fn sync_claims(key: KeySet, recipient: Address) -> Result<(), CliError> {
    let client = get_client()?;
    let recipient = convert_address(recipient);
    client.sync_claims(key, recipient).await?;
    Ok(())
}
