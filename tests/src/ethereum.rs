use ethers::{abi::AbiEncode, prelude::*};

pub async fn get_balance(
    l1_rpc_url: &str,
    address: Address,
) -> Result<U256, Box<dyn std::error::Error>> {
    // Connect to Ethereum network
    let provider = Provider::<Http>::try_from(l1_rpc_url)?;

    // Get balance
    let balance = provider.get_balance(address, None).await?;
    Ok(balance)
}

pub async fn transfer_eth_on_ethereum(
    l1_rpc_url: &str,
    private_key: &str,
    recipient: Address,
    amount: U256,
) -> Result<TransactionReceipt, Box<dyn std::error::Error>> {
    // Connect to Ethereum network
    let provider = Provider::<Http>::try_from(l1_rpc_url)?;
    let chain_id = provider.get_chainid().await?;

    // Create wallet from private key
    let wallet: LocalWallet = private_key
        .parse::<LocalWallet>()?
        .with_chain_id(chain_id.as_u64());

    // connect the wallet to the provider
    let client = SignerMiddleware::new(provider, wallet);

    // Create the transaction
    let tx = TransactionRequest::new().to(recipient).value(amount);

    // Send batch transaction
    let pending_tx = client.send_transaction(tx, None).await?;
    println!("tx hash: 0x{}", pending_tx.tx_hash().encode_hex());

    let receipt = pending_tx
        .await?
        .ok_or_else(|| anyhow::anyhow!("tx dropped from mempool"))?;
    let tx = client.get_transaction(receipt.transaction_hash).await?;

    println!("Sent tx: {}\n", serde_json::to_string(&tx)?);
    println!("Tx receipt: {}", serde_json::to_string(&receipt)?);

    Ok(receipt)
}

pub async fn transfer_eth_batch_on_ethereum(
    _l1_rpc_url: &str,
    _private_key: &str,
    _recipients: &[Address],
    _amount: U256,
) -> Result<TransactionReceipt, Box<dyn std::error::Error>> {
    // // Connect to Ethereum network
    // let provider = Provider::<Http>::try_from(l1_rpc_url)?;
    // let chain_id = provider.get_chainid().await?;

    // // Create wallet from private key
    // let wallet: LocalWallet = private_key
    //     .parse::<LocalWallet>()?
    //     .with_chain_id(chain_id.as_u64());

    // // connect the wallet to the provider
    // let client = SignerMiddleware::new(provider, wallet);

    // // Create the transaction
    // let tx = TransactionRequest::new().to(recipients[0]).value(amount);
    // let mut multicall = Multicall::new(client.clone(), None).await?;
    // for recipient in recipients {
    //     let tx = TransactionRequest::new().to(*recipient).value(amount);

    //     let call: TypedTransaction = tx.into();
    //     // let contract_call = ContractCall::from(call);
    //     // let call = Multicall3Calls::Aggregate3Value(Aggregate3ValueCall::new(
    //     //     client.clone(),
    //     //     tx.to.unwrap(),
    //     //     tx.data.unwrap_or_default(),
    //     //     tx.value.unwrap_or_default(),
    //     // ));
    //     // multicall.add_call(call.into(), false);
    // }

    // // Send batch transaction
    // let pending_tx = multicall.send().await?;
    // println!("tx hash: 0x{}", pending_tx.tx_hash().encode_hex());

    // let receipt = pending_tx
    //     .await?
    //     .ok_or_else(|| anyhow::anyhow!("tx dropped from mempool"))?;
    // let tx = client.get_transaction(receipt.transaction_hash).await?;

    // println!("Sent tx: {}\n", serde_json::to_string(&tx)?);
    // println!("Tx receipt: {}", serde_json::to_string(&receipt)?);

    // Ok(receipt)
    todo!()
}
