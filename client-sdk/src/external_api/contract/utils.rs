use std::sync::Arc;

#[cfg(not(target_arch = "wasm32"))]
use std::collections::HashMap;

use ethers::{
    core::k256::{ecdsa::SigningKey, SecretKey},
    middleware::SignerMiddleware,
    providers::{Http, Middleware as _, Provider},
    signers::{Signer as _, Wallet},
    types::{Address, BlockNumber, H256, U256},
};

use crate::external_api::utils::retry::with_retry;

use super::error::BlockchainError;

async fn get_provider(rpc_url: &str) -> Result<Provider<Http>, BlockchainError> {
    let provider = Provider::<Http>::try_from(rpc_url)
        .map_err(|_| BlockchainError::ParseError("Failed to parse RPC_URL".to_string()))?;
    Ok(provider)
}

pub async fn get_client(rpc_url: &str) -> Result<Arc<Provider<Http>>, BlockchainError> {
    Ok(Arc::new(get_provider(rpc_url).await?))
}

pub fn get_wallet(chain_id: u64, private_key: H256) -> Wallet<SigningKey> {
    let key = SecretKey::from_bytes(private_key.as_bytes().into()).unwrap();
    Wallet::from(key).with_chain_id(chain_id)
}

pub fn get_address(chain_id: u64, private_key: H256) -> Address {
    get_wallet(chain_id, private_key).address()
}

pub async fn get_gas_price(rpc_url: &str) -> Result<U256, BlockchainError> {
    let client = get_client(rpc_url).await?;
    let gas_price = with_retry(|| async { client.get_gas_price().await })
        .await
        .map_err(|_| BlockchainError::RPCError("failed to get gas price".to_string()))?;
    Ok(gas_price)
}

pub async fn get_client_with_signer(
    rpc_url: &str,
    chain_id: u64,
    private_key: H256,
) -> Result<SignerMiddleware<Provider<Http>, Wallet<SigningKey>>, BlockchainError> {
    let provider = get_provider(rpc_url).await?;
    let wallet = get_wallet(chain_id, private_key);
    let client = SignerMiddleware::new(provider, wallet);
    Ok(client)
}

pub async fn get_base_fee(rpc_url: &str) -> Result<U256, BlockchainError> {
    let client = get_client(rpc_url).await?;
    let latest_block = with_retry(|| async { client.get_block(BlockNumber::Latest).await })
        .await
        .map_err(|_| BlockchainError::RPCError("failed to get latest block".to_string()))?
        .expect("latest block not found");
    let base_fee = latest_block
        .base_fee_per_gas
        .ok_or(BlockchainError::BlockBaseFeeNotFound)?;
    Ok(base_fee)
}

pub async fn estimate_eip1559_fees(rpc_url: &str) -> Result<(U256, U256), BlockchainError> {
    let client = get_client(rpc_url).await?;
    let (max_fee_per_gas, max_priority_fee_per_gas) =
        with_retry(|| async { client.estimate_eip1559_fees(None).await })
            .await
            .map_err(|_| {
                BlockchainError::RPCError("failed to get max priority fee per gas".to_string())
            })?;
    Ok((max_fee_per_gas, max_priority_fee_per_gas))
}

pub async fn get_latest_block_number(rpc_url: &str) -> Result<u64, BlockchainError> {
    let client = get_client(rpc_url).await?;
    let block_number = with_retry(|| async { client.get_block_number().await })
        .await
        .map_err(|_| BlockchainError::RPCError("failed to get block number".to_string()))?;
    Ok(block_number.as_u64())
}

pub async fn get_eth_balance(rpc_url: &str, address: Address) -> Result<U256, BlockchainError> {
    let client = get_client(rpc_url).await?;
    let balance = with_retry(|| async { client.get_balance(address, None).await })
        .await
        .map_err(|_| BlockchainError::RPCError("failed to get block number".to_string()))?;
    Ok(balance)
}

pub async fn get_transaction(
    rpc_url: &str,
    tx_hash: H256,
) -> Result<Option<ethers::types::Transaction>, BlockchainError> {
    let client = get_client(rpc_url).await?;
    let tx = with_retry(|| async { client.get_transaction(tx_hash).await })
        .await
        .map_err(|_| BlockchainError::RPCError("failed to get transaction".to_string()))?;
    Ok(tx)
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn get_batch_transaction(
    rpc_url: &str,
    tx_hashes: &[H256],
) -> Result<Vec<ethers::types::Transaction>, BlockchainError> {
    use crate::external_api::utils::time::sleep_for;
    use std::collections::HashMap;

    let mut target_tx_hashes = tx_hashes.to_vec();
    let mut fetched_txs = HashMap::new();
    let mut retry_count = 0;
    let max_tries = std::env::var("MAX_TRIES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10);
    while !target_tx_hashes.is_empty() {
        let (partial_fetched_txs, failed_tx_hashes) =
            get_batch_transaction_inner(rpc_url, &target_tx_hashes).await?;
        fetched_txs.extend(partial_fetched_txs);
        if failed_tx_hashes.is_empty() {
            break;
        }
        log::warn!(
            "Fetched {} transactions, failed {}",
            fetched_txs.len(),
            failed_tx_hashes.len()
        );
        target_tx_hashes = failed_tx_hashes;
        retry_count += 1;
        if retry_count > max_tries {
            return Err(BlockchainError::TxNotFoundBatch);
        }
        sleep_for(2).await;
    }
    let mut txs = Vec::new();
    for tx_hash in tx_hashes {
        txs.push(fetched_txs.get(tx_hash).unwrap().clone());
    }
    Ok(txs)
}

#[cfg(not(target_arch = "wasm32"))]
async fn get_batch_transaction_inner(
    rpc_url: &str,
    tx_hashes: &[H256],
) -> Result<(HashMap<H256, ethers::types::Transaction>, Vec<H256>), BlockchainError> {
    use crate::external_api::contract::utils::get_transaction;
    use std::env;
    use tokio::task::JoinSet;
    let mut join_set = JoinSet::new();
    let max_parallel_requests = env::var("MAX_PARALLEL_REQUESTS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(20);
    let semaphore = Arc::new(tokio::sync::Semaphore::new(max_parallel_requests));
    for &tx_hash in tx_hashes {
        let permit = Arc::clone(&semaphore);
        let rpc_url = rpc_url.to_string();
        join_set.spawn(async move {
            let _permit = permit.acquire().await.expect("Semaphore is never closed");
            let tx = get_transaction(&rpc_url, tx_hash)
                .await?
                .ok_or(BlockchainError::TxNotFound(tx_hash))?;
            Ok::<_, BlockchainError>((tx_hash, tx))
        });
    }

    let mut fetched_txs = HashMap::new();
    let mut failed_tx_hashes = Vec::new();
    while let Some(result) = join_set.join_next().await {
        match result {
            Ok(Ok((tx_hash, tx))) => {
                fetched_txs.insert(tx_hash, tx);
            }
            Ok(Err(e)) => {
                if let BlockchainError::TxNotFound(tx_hash) = e {
                    failed_tx_hashes.push(tx_hash);
                } else {
                    return Err(e);
                }
            }
            Err(e) => return Err(BlockchainError::JoinError(e.to_string())),
        }
    }
    Ok((fetched_txs, failed_tx_hashes))
}
