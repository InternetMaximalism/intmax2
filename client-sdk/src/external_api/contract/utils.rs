use alloy::{
    network::EthereumWallet,
    primitives::B256,
    providers::{
        fillers::{FillProvider, JoinFill, WalletFiller},
        utils::JoinedRecommendedFillers,
        ProviderBuilder,
    },
    rpc::client::RpcClient,
    signers::local::PrivateKeySigner,
    transports::{
        http::Http,
        layers::{FallbackLayer, RetryBackoffLayer},
    },
};
use reqwest::Url;
use tower::ServiceBuilder;

pub type NormalProvider = FillProvider<JoinedRecommendedFillers, alloy::providers::RootProvider>;

pub type ProviderWithSigner = FillProvider<
    JoinFill<JoinedRecommendedFillers, WalletFiller<EthereumWallet>>,
    alloy::providers::RootProvider,
>;
use super::error::BlockchainError;

pub fn get_provider(rpc_urls: &[String]) -> Result<NormalProvider, BlockchainError> {
    let retry_layer = RetryBackoffLayer::new(5, 1000, 100);
    let transports = rpc_urls
        .iter()
        .map(|url| {
            let url: Url = url.parse().map_err(|e| {
                BlockchainError::ParseError(format!("Failed to parse URL {}: {}", url, e))
            })?;
            Ok(Http::new(url))
        })
        .collect::<Result<Vec<_>, BlockchainError>>()?;
    let fallback_layer =
        FallbackLayer::default().with_active_transport_count(transports.len().try_into().unwrap());
    let transport = ServiceBuilder::new()
        .layer(fallback_layer)
        .service(transports);
    let client = RpcClient::builder()
        .layer(retry_layer)
        .transport(transport, false);
    let provider = ProviderBuilder::new().connect_client(client);
    Ok(provider)
}

pub fn get_provider_with_signer(provider: NormalProvider, private_key: B256) -> ProviderWithSigner {
    let signer = PrivateKeySigner::from_bytes(&private_key).unwrap();
    let wallet = EthereumWallet::new(signer);
    let wallet_filler = WalletFiller::new(wallet);
    provider.join_with(wallet_filler)
}

// #[cfg(not(target_arch = "wasm32"))]
// pub async fn get_batch_transaction(
//     rpc_url: &str,
//     tx_hashes: &[H256],
// ) -> Result<Vec<ethers::types::Transaction>, BlockchainError> {
//     use crate::external_api::utils::time::sleep_for;
//     use std::collections::HashMap;

//     let mut target_tx_hashes = tx_hashes.to_vec();
//     let mut fetched_txs = HashMap::new();
//     let mut retry_count = 0;
//     let max_tries = std::env::var("MAX_TRIES")
//         .ok()
//         .and_then(|s| s.parse().ok())
//         .unwrap_or(10);
//     while !target_tx_hashes.is_empty() {
//         let (partial_fetched_txs, failed_tx_hashes) =
//             get_batch_transaction_inner(rpc_url, &target_tx_hashes).await?;
//         fetched_txs.extend(partial_fetched_txs);
//         if failed_tx_hashes.is_empty() {
//             break;
//         }
//         log::warn!(
//             "Fetched {} transactions, failed {}",
//             fetched_txs.len(),
//             failed_tx_hashes.len()
//         );
//         target_tx_hashes = failed_tx_hashes;
//         retry_count += 1;
//         if retry_count > max_tries {
//             return Err(BlockchainError::TxNotFoundBatch);
//         }
//         sleep_for(2).await;
//     }
//     let mut txs = Vec::new();
//     for tx_hash in tx_hashes {
//         txs.push(fetched_txs.get(tx_hash).unwrap().clone());
//     }
//     Ok(txs)
// }

// #[cfg(not(target_arch = "wasm32"))]
// async fn get_batch_transaction_inner(
//     rpc_url: &str,
//     tx_hashes: &[H256],
// ) -> Result<(HashMap<H256, ethers::types::Transaction>, Vec<H256>), BlockchainError> {
//     use crate::external_api::contract::utils::get_transaction;
//     use std::env;
//     use tokio::task::JoinSet;
//     let mut join_set = JoinSet::new();
//     let max_parallel_requests = env::var("MAX_PARALLEL_REQUESTS")
//         .ok()
//         .and_then(|s| s.parse().ok())
//         .unwrap_or(20);
//     let semaphore = Arc::new(tokio::sync::Semaphore::new(max_parallel_requests));
//     for &tx_hash in tx_hashes {
//         let permit = Arc::clone(&semaphore);
//         let rpc_url = rpc_url.to_string();
//         join_set.spawn(async move {
//             let _permit = permit.acquire().await.expect("Semaphore is never closed");
//             let tx = get_transaction(&rpc_url, tx_hash)
//                 .await?
//                 .ok_or(BlockchainError::TxNotFound(tx_hash))?;
//             Ok::<_, BlockchainError>((tx_hash, tx))
//         });
//     }

//     let mut fetched_txs = HashMap::new();
//     let mut failed_tx_hashes = Vec::new();
//     while let Some(result) = join_set.join_next().await {
//         match result {
//             Ok(Ok((tx_hash, tx))) => {
//                 fetched_txs.insert(tx_hash, tx);
//             }
//             Ok(Err(e)) => {
//                 if let BlockchainError::TxNotFound(tx_hash) = e {
//                     failed_tx_hashes.push(tx_hash);
//                 } else {
//                     return Err(e);
//                 }
//             }
//             Err(e) => return Err(BlockchainError::JoinError(e.to_string())),
//         }
//     }
//     Ok((fetched_txs, failed_tx_hashes))
// }
