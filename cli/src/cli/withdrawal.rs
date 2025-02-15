use intmax2_client_sdk::client::{
    fee_payment::generate_withdrawal_transfers, sync::utils::generate_salt,
};
use intmax2_zkp::{
    common::{generic_address::GenericAddress, signature::key_set::KeySet, transfer::Transfer},
    ethereum_types::{address::Address, u256::U256},
};

use super::{client::get_client, error::CliError, send::send_transfers};

pub async fn send_withdrawal(
    key: KeySet,
    to: Address,
    amount: U256,
    token_index: u32,
    fee_token_index: u32,
    with_claim_fee: bool,
) -> Result<(), CliError> {
    let client = get_client()?;
    let withdrawal_transfer = Transfer {
        recipient: GenericAddress::from_address(to),
        token_index,
        amount,
        salt: generate_salt(),
    };
    let (transfers, payment_memos) = generate_withdrawal_transfers(
        &client.withdrawal_server,
        &client.withdrawal_contract,
        &withdrawal_transfer,
        fee_token_index,
        with_claim_fee,
    )
    .await?;
    send_transfers(key, &transfers, payment_memos, fee_token_index).await?;
    Ok(())
}
