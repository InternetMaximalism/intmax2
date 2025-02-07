// #[derive(Debug, Clone, Deserialize)]
// pub struct EnvVar {
//     pub master_mnemonic: String,
//     pub private_key: String,
//     pub num_of_recipients: Option<u32>,
//     pub recipient_offset: Option<u32>,
//     pub balance_prover_base_url: String,
// }

// #[derive(Debug, Default, Clone, Copy)]
// struct TransactionStats {
//     success_count: u32,
//     failure_count: u32,
// }

// async fn process_account(key: KeySet, transfers: &[TransferInput]) -> Result<(), CliError> {
//     transfer(key, transfers).await?;
//     tokio::time::sleep(Duration::from_secs(20)).await;
//     wait_for_balance_synchronization(key, Duration::from_secs(5)).await
// }

// async fn account_loop(
//     stats: Arc<Mutex<HashMap<String, TransactionStats>>>,
//     semaphore: Arc<Semaphore>,
//     key: KeySet,
//     transfers: &[TransferInput],
//     cool_down_seconds: u64,
// ) {
//     loop {
//         if account_id < config.num_of_recipients.unwrap_or(10) {
//             continue;
//         }

//         let permit = semaphore.clone().acquire_owned().await.unwrap();
//         log::trace!("Starting transfer from {}", key.pubkey);
//         let result = process_account(key, transfers).await;

//         let mut stats = stats.lock().unwrap();
//         let entry = stats
//             .entry(key.pubkey.to_hex())
//             .or_insert(TransactionStats::default());

//         match result {
//             Ok(_) => entry.success_count += 1,
//             Err(_) => entry.failure_count += 1,
//         }

//         drop(permit);

//         println!(
//             "[Account {}] Cooldown: Waiting {} seconds before next transaction...\n",
//             key.pubkey.to_hex(),
//             cool_down_seconds
//         );
//         tokio::time::sleep(Duration::from_secs(cool_down_seconds)).await;
//     }
// }

#[tokio::test]
async fn test_soak_block_generation() -> Result<(), Box<dyn std::error::Error>> {
    // dotenv::dotenv()?;
    // dotenv::from_path("../cli/.env")?;
    // let config = envy::from_env::<EnvVar>().unwrap();
    // env_logger::builder()
    //     .filter_level(log::LevelFilter::Info)
    //     .init();

    // log::debug!(
    //     "block_builder_base_url: {:?}",
    //     config.balance_prover_base_url
    // );

    // let master_mnemonic_phrase = config.master_mnemonic;
    // let num_of_recipients = config.num_of_recipients.unwrap_or(1);
    // log::debug!("Number of recipients: {}", num_of_recipients);
    // if num_of_recipients > 128 {
    //     return Err("Number of recipients must be less than or equal to 128".into());
    // }

    // let offset = 0;
    // let intmax_senders = derive_intmax_keys(&master_mnemonic_phrase, num_of_recipients, offset)?;

    // let intmax_recipient = {
    //     let options = MnemonicToPrivateKeyOptions {
    //         account_index: 1,
    //         address_index: 0,
    //     };
    //     let private_key = mnemonic_to_private_key(&master_mnemonic_phrase, options)?;

    //     generate_intmax_account_from_eth_key(private_key)
    // };

    // // multiple senders -> receiver (simultaneously)
    // let transfer_input = TransferInput {
    //     recipient: intmax_recipient.pubkey.to_hex(),
    //     amount: 10u128,
    //     token_index: ETH_TOKEN_INDEX,
    // };
    // let transfers = [transfer_input];

    // log::info!("Transferring from recipients to sender...");
    // tokio::time::sleep(Duration::from_secs(1)).await;

    // let semaphore = Arc::new(Semaphore::new(config.concurrent_limit.unwrap_or(5)));
    // let stats = Arc::new(Mutex::new(HashMap::new()));

    // let mut tasks = vec![];

    // for account_id in 0..100 {
    //     let semaphore_clone = Arc::clone(&semaphore);
    //     let stats_clone = Arc::clone(&stats);
    //     let key = intmax_senders[account_id as usize];

    //     let task = tokio::spawn(async move {
    //         account_loop(
    //             stats_clone,
    //             semaphore_clone,
    //             key,
    //             &transfers,
    //             config.cool_down_seconds.unwrap_or(30),
    //         )
    //         .await;
    //     });

    //     tasks.push(task);
    // }

    // join_all(tasks).await.into_iter().collect()

    Ok(())
}
