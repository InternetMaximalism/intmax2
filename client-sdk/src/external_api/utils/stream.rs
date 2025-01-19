use bytes::{Bytes, BytesMut};
use futures_util::{stream, Stream, StreamExt as _};
use intmax2_interfaces::api::error::ServerError;
use serde::{de::DeserializeOwned, Serialize};

use super::{query::handle_response, retry::with_retry};

pub async fn stream_post_upload<B: Serialize, R: DeserializeOwned>(
    base_url: &str,
    endpoint: &str,
    body: &B,
) -> Result<R, ServerError> {
    let url = format!("{}{}", base_url, endpoint);
    let client = reqwest::Client::new();
    let data = bincode::serialize(body).map_err(|e| ServerError::SerializeError(e.to_string()))?;
    let response = with_retry(|| async {
        let stream = create_stream(&data);
        client
            .post(&url)
            .header("Content-Type", "application/octet-stream")
            .body(reqwest::Body::wrap_stream(stream))
            .send()
            .await
    })
    .await
    .map_err(|e| ServerError::NetworkError(e.to_string()))?;
    handle_response(response, &url, &None).await
}

pub async fn stream_post_download<B: Serialize, R: DeserializeOwned>(
    base_url: &str,
    endpoint: &str,
    body: &B,
) -> Result<R, ServerError> {
    let url = format!("{}{}", base_url, endpoint);
    let client = reqwest::Client::new();
    let response = with_retry(|| async { client.post(&url).json(body).send().await })
        .await
        .map_err(|e| ServerError::NetworkError(e.to_string()))?;
    let mut stream = response.bytes_stream();
    let mut bytes = BytesMut::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| ServerError::StreamError(e.to_string()))?;
        bytes.extend_from_slice(&chunk);
    }
    let response: R =
        bincode::deserialize(&bytes).map_err(|e| ServerError::SerializeError(e.to_string()))?;
    Ok(response)
}

fn create_stream(data: &[u8]) -> impl Stream<Item = anyhow::Result<Bytes>> {
    const CHUNK_SIZE: usize = 1 << 20; // 1MB
    let chunks: Vec<_> = data
        .chunks(CHUNK_SIZE)
        .map(|chunk| chunk.to_vec())
        .collect();
    stream::iter(chunks).map(|chunk| Ok(Bytes::from(chunk)))
}
