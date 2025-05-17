use alloy::primitives::B256;
use intmax2_client_sdk::{
    self,
    client::{client::Client, key_from_eth::generate_intmax_account_from_eth_key},
    external_api::utils::time::sleep_for,
};
use intmax2_zkp::common::{salt::Salt, transfer::Transfer};

use crate::{
    config::TestConfig,
    send::send_transfers,
    utils::{get_balance_on_intmax, print_info},
};

pub async fn transfer_loop(
    config: &TestConfig,
    client: &Client,
    eth_private_key: B256,
) -> anyhow::Result<()> {
    print_info(client, eth_private_key).await?;
    let key = generate_intmax_account_from_eth_key(eth_private_key);

    let balance = get_balance_on_intmax(client, key).await?;
    if balance < 100.into() {
        log::warn!("Insufficient balance to perform transfers");
        return Ok(());
    }

    loop {
        let transfer = Transfer {
            recipient: key.pubkey.into(),
            token_index: 0,
            amount: 1.into(),
            salt: Salt::rand(&mut rand::thread_rng()),
        };
        let mut retries = 0;
        loop {
            if retries >= config.tx_resend_retries {
                return Err(anyhow::anyhow!(
                    "Failed to send transfer after {} retries",
                    retries
                ));
            }
            let result = send_transfers(config, client, key, &[transfer], &[], 0).await;
            match result {
                Ok(_) => break,
                Err(e) => {
                    log::warn!("Failed to send transfer: {}", e);
                }
            }
            log::warn!("Retrying...");
            sleep_for(config.tx_resend_interval).await;
            retries += 1;
        }
        client.sync(key).await?;
        log::info!(
            "Transfer completed. Sleeping for {} seconds",
            config.transfer_loop_wait_time
        );
        sleep_for(config.transfer_loop_wait_time).await;
    }
}
