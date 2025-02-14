// pub async fn withdrawal(key: KeySet, to: String, fee_token_index: u32) -> Result<(), CliError> {
//     let mut rng = rand::thread_rng();
//     if transfer_inputs.len() > NUM_TRANSFERS_IN_TX - 1 {
//         return Err(CliError::TooManyTransfer(transfer_inputs.len()));
//     }

//     let transfers = transfer_inputs
//         .iter()
//         .map(|input| {
//             let recipient = parse_generic_address(&input.recipient)
//                 .map_err(|e| CliError::ParseError(format!("Failed to parse recipient: {}", e)))?;
//             let amount = convert_u256(input.amount.into());
//             let token_index = input.token_index;
//             let salt = Salt::rand(&mut rng);
//             Ok(Transfer {
//                 recipient,
//                 amount,
//                 token_index,
//                 salt,
//             })
//         })
//         .collect::<Result<Vec<_>, CliError>>()?;

//     let env = envy::from_env::<EnvVar>()?;
//     let client = get_client()?;

//     // override block builder base url if it is set in the env
//     let block_builder_url = if let Some(block_builder_base_url) = env.block_builder_base_url {
//         block_builder_base_url.to_string()
//     } else {
//         // get block builder info
//         let indexer = IndexerClient::new(&env.indexer_base_url.to_string());
//         let block_builder_info = indexer.get_block_builder_info().await?;
//         if block_builder_info.is_empty() {
//             return Err(CliError::UnexpectedError(
//                 "Block builder info is empty".to_string(),
//             ));
//         }
//         block_builder_info.first().unwrap().url.clone()
//     };

//     let fee_quote = client
//         .quote_fee(&block_builder_url, key.pubkey, fee_token_index)
//         .await?;
//     if let Some(fee) = &fee_quote.fee {
//         log::info!("beneficiary: {}", fee_quote.beneficiary.unwrap().to_hex());
//         log::info!("Fee: {} (token# {})", fee.amount, fee.token_index);
//     }
//     if let Some(collateral_fee) = &fee_quote.collateral_fee {
//         log::info!(
//             "Collateral Fee: {} (token# {})",
//             collateral_fee.amount,
//             collateral_fee.token_index
//         );
//     }
//     let memo = client
//         .send_tx_request(
//             &block_builder_url,
//             key,
//             transfers,
//             vec![],
//             fee_quote.beneficiary,
//             fee_quote.fee,
//             fee_quote.collateral_fee,
//         )
//         .await?;

//     let is_registration_block = memo.is_registration_block;
//     let tx = memo.tx;

//     log::info!("Waiting for block builder to build the block");
//     tokio::time::sleep(std::time::Duration::from_secs(
//         env.block_builder_query_wait_time,
//     ))
//     .await;

//     let proposal = client
//         .query_proposal(&block_builder_url, key, is_registration_block, tx)
//         .await?;

//     log::info!("Finalizing tx");
//     client
//         .finalize_tx(&block_builder_url, key, &memo, &proposal)
//         .await?;

//     Ok(())
// }
