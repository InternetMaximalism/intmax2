use alloy::primitives::B256;
use anyhow::Context;
use intmax2_client_sdk::{
    client::client::Client,
    external_api::{
        contract::{convert::convert_u256_to_intmax, utils::get_address_from_private_key},
        utils::time::sleep_for,
    },
};

use crate::{
    config::TestConfig,
    deposit::single_deposit,
    utils::{calculate_balance_with_gas_deduction, print_info},
    withdrawal::single_withdrawal,
};

pub async fn bridge_loop(
    config: &TestConfig,
    client: &Client,
    eth_private_key: B256,
    from_withdrawal: bool,
) -> anyhow::Result<()> {
    print_info(client, eth_private_key).await?;

    if from_withdrawal {
        single_withdrawal(config, client, eth_private_key, false, true)
            .await
            .context("Failed to perform withdrawal")?;
    }

    loop {
        let depositor = get_address_from_private_key(eth_private_key);
        let gas_limit = 200000;
        let deposit_amount = calculate_balance_with_gas_deduction(
            &client.liquidity_contract.provider,
            depositor,
            2,
            gas_limit,
        )
        .await?;
        let deposit_amount = convert_u256_to_intmax(deposit_amount);

        single_deposit(config, client, eth_private_key, deposit_amount)
            .await
            .context("Failed to perform deposit")?;
        log::info!(
            "Deposit completed. Sleeping for {} seconds",
            config.bridge_loop_intmax_wait_time
        );
        sleep_for(config.bridge_loop_intmax_wait_time).await;

        single_withdrawal(config, client, eth_private_key, false, true)
            .await
            .context("Failed to perform withdrawal")?;
        log::info!(
            "Withdrawal completed. Sleeping for {} seconds",
            config.bridge_loop_eth_wait_time
        );
        sleep_for(config.bridge_loop_eth_wait_time).await;
    }
}
