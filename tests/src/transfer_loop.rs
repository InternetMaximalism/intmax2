use alloy::primitives::B256;
use intmax2_client_sdk::{
    self,
    client::client::{Client, GenericRecipient, TransferRequest},
    external_api::utils::time::sleep_for,
};
use intmax2_interfaces::utils::address::IntmaxAddress;

use crate::{
    config::TestConfig,
    send::send_transfers,
    utils::{get_balance_on_intmax, get_keypair_from_eth_key, print_info},
};

pub async fn transfer_loop(
    config: &TestConfig,
    client: &Client,
    eth_private_key: B256,
) -> anyhow::Result<()> {
    print_info(client, eth_private_key).await?;
    let key_pair = get_keypair_from_eth_key(eth_private_key);
    let address = IntmaxAddress::from_keypair(client.config.network, &key_pair);

    let balance = get_balance_on_intmax(client, key_pair.into()).await?;
    if balance < 100.into() {
        log::warn!("Insufficient balance to perform transfers");
        return Ok(());
    }

    loop {
        let transfer = TransferRequest {
            recipient: GenericRecipient::IntmaxAddress(address),
            token_index: 0,
            amount: 1.into(),
            description: None,
        };
        let mut retries = 0;
        loop {
            if retries >= config.tx_resend_retries {
                return Err(anyhow::anyhow!(
                    "Failed to send transfer after {} retries",
                    retries
                ));
            }
            let result =
                send_transfers(config, client, key_pair, &[transfer.clone()], &[], 0).await;
            match result {
                Ok(_) => break,
                Err(e) => {
                    log::warn!("Failed to send transfer: {e}");
                }
            }
            log::warn!("Retrying...");
            sleep_for(config.tx_resend_interval).await;
            retries += 1;
        }
        client.sync(key_pair.into()).await?;
        log::info!(
            "Transfer completed. Sleeping for {} seconds",
            config.transfer_loop_wait_time
        );
        sleep_for(config.transfer_loop_wait_time).await;
    }
}
