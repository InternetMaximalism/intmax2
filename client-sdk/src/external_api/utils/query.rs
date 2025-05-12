use intmax2_interfaces::api::error::ServerError;
use reqwest::{Response, Url};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use super::{debug::is_debug_mode, retry::with_retry};

#[derive(Debug, Deserialize)]
struct ErrorResponse {
    error: String,
    #[serde(default)]
    message: Option<String>,
}

pub async fn post_request<B: Serialize, R: DeserializeOwned>(
    base_url: &str,
    endpoint: &str,
    body: Option<&B>,
) -> Result<R, ServerError> {
    let url = format!("{}{}", base_url, endpoint);
    let _ = Url::parse(&url)
        .map_err(|e| ServerError::MalformedUrl(format!("Failed to parse URL {}: {}", url, e)))?;
    let client = reqwest::Client::new();
    let response = if let Some(body) = body {
        with_retry(|| async { client.post(&url).json(body).send().await }).await
    } else {
        with_retry(|| async { client.post(&url).send().await }).await
    }
    .map_err(|e| ServerError::NetworkError(e.to_string()))?;

    // Serialize the body to a string for logging
    let body_str = if let Some(body) = &body {
        let body_str = serde_json::to_string(body)
            .map_err(|e| ServerError::SerializeError(format!("Failed to serialize body: {}", e)))?;
        Some(body_str)
    } else {
        None
    };
    if is_debug_mode() {
        let body_size = body_str.as_ref().map(|s| s.len()).unwrap_or(0);
        log::info!("POST request url: {} body size: {} bytes", url, body_size);
    }
    handle_response(response, &url, &body_str).await
}

pub async fn get_request<Q, R>(
    base_url: &str,
    endpoint: &str,
    query: Option<Q>,
) -> Result<R, ServerError>
where
    Q: Serialize,
    R: DeserializeOwned,
{
    let mut url = format!("{}{}", base_url, endpoint);
    let _ = Url::parse(&url)
        .map_err(|e| ServerError::MalformedUrl(format!("Failed to parse URL {}: {}", url, e)))?;
    let query_str = query
        .as_ref()
        .map(|q| {
            serde_qs::to_string(&q).map_err(|e| {
                ServerError::SerializeError(format!("Failed to serialize query: {}", e))
            })
        })
        .transpose()?;
    if query_str.is_some() {
        url = format!("{}?{}", url, query_str.as_ref().unwrap());
    }
    let client = reqwest::Client::new();
    let response = with_retry(|| async { client.get(&url).send().await })
        .await
        .map_err(|e| ServerError::NetworkError(e.to_string()))?;
    if is_debug_mode() {
        log::info!("GET request url: {}", url);
    }
    handle_response(response, &url, &query_str).await
}

async fn handle_response<R: DeserializeOwned>(
    response: Response,
    url: &str,
    request_str: &Option<String>,
) -> Result<R, ServerError> {
    let status = response.status();
    if !status.is_success() {
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Failed to read error response".to_string());
        let error_message = match serde_json::from_str::<ErrorResponse>(&error_text) {
            Ok(error_resp) => error_resp.message.unwrap_or(error_resp.error),
            Err(_) => error_text,
        };
        let abr_request = if is_debug_mode() {
            // full request string
            request_str.clone().unwrap_or_default()
        } else {
            // Truncate the request string to 500 characters if it is too long
            request_str
                .as_ref()
                .map(|s| s.chars().take(500).collect::<String>())
                .unwrap_or_default()
        };
        return Err(ServerError::ServerError(
            status.into(),
            error_message,
            url.to_string(),
            abr_request,
        ));
    }
    response
        .json::<R>()
        .await
        .map_err(|e| ServerError::DeserializationError(e.to_string()))
}
