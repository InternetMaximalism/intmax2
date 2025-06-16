use std::time::Duration;

use intmax2_interfaces::api::error::ServerError;
use reqwest::{header, Client, Response, Url};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::external_api::utils::retry::with_retry;

/// Timeout for reqwest requests.
/// Because WASM only accepts timeout in the request builder, not the client builder,
/// we set a timeout of 30 seconds for each request.
pub const REQWEST_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Deserialize)]
struct ErrorResponse {
    error: String,
    #[serde(default)]
    message: Option<String>,
}

pub fn build_client() -> Client {
    reqwest::Client::builder()
        .build()
        .expect("Failed to build reqwest client")
}

pub async fn post_request<B: Serialize, R: DeserializeOwned>(
    client: &Client,
    base_url: &str,
    endpoint: &str,
    body: Option<&B>,
) -> Result<R, ServerError> {
    post_request_with_bearer_token(client, base_url, endpoint, None, body).await
}

pub async fn post_request_with_bearer_token<B: Serialize, R: DeserializeOwned>(
    client: &Client,
    base_url: &str,
    endpoint: &str,
    bearer_token: Option<String>,
    body: Option<&B>,
) -> Result<R, ServerError> {
    let url = format!("{base_url}{endpoint}");
    let _ = Url::parse(&url)
        .map_err(|e| ServerError::MalformedUrl(format!("Failed to parse URL {url}: {e}")))?;
    let mut request = client.post(url.clone()).timeout(REQWEST_TIMEOUT);
    if let Some(token) = bearer_token {
        request = request.header(header::AUTHORIZATION, token);
    }
    if let Some(body) = body {
        request = request.json(body);
    }
    let response = with_retry(|| async { request.try_clone().unwrap().send().await })
        .await
        .map_err(|e| ServerError::NetworkError(e.to_string()))?;

    // Serialize the body to a string for logging
    let body_str = if let Some(body) = &body {
        let body_str = serde_json::to_string(body)
            .map_err(|e| ServerError::SerializeError(format!("Failed to serialize body: {e}")))?;
        Some(body_str)
    } else {
        None
    };
    let body_size = body_str.as_ref().map(|s| s.len()).unwrap_or(0);
    log::debug!("POST request url: {url} body size: {body_size} bytes");

    handle_response(response, &url, &body_str).await
}

pub async fn get_request<Q, R>(
    client: &Client,
    base_url: &str,
    endpoint: &str,
    query: Option<Q>,
) -> Result<R, ServerError>
where
    Q: Serialize,
    R: DeserializeOwned,
{
    let mut url = format!("{base_url}{endpoint}");
    let _ = Url::parse(&url)
        .map_err(|e| ServerError::MalformedUrl(format!("Failed to parse URL {url}: {e}")))?;
    let query_str = query
        .as_ref()
        .map(|q| {
            serde_qs::to_string(&q)
                .map_err(|e| ServerError::SerializeError(format!("Failed to serialize query: {e}")))
        })
        .transpose()?;
    if query_str.is_some() {
        url = format!("{}?{}", url, query_str.as_ref().unwrap());
    }
    let response = with_retry(|| async { client.get(&url).timeout(REQWEST_TIMEOUT).send().await })
        .await
        .map_err(|e| ServerError::NetworkError(e.to_string()))?;
    log::debug!("GET request url: {url}");
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
        let abr_request = if log::log_enabled!(log::Level::Debug) {
            // full request string
            request_str.clone().unwrap_or_default()
        } else {
            String::default()
        };
        return Err(ServerError::ResponseError(format!(
            "Request to url:{url} failed with status:{status}, request:{abr_request}, error:{error_message}"
        )));
    }

    let response_text = response.text().await.map_err(|e| {
        ServerError::ResponseDeserializationError(format!("Failed to read response: {e}"))
    })?;

    match serde_json::from_str::<R>(&response_text) {
        Ok(result) => Ok(result),
        Err(e) => {
            let abr_response = if log::log_enabled!(log::Level::Debug) {
                // full request string
                response_text
            } else {
                // Truncate the response string to 500 characters if it is too long
                response_text.chars().take(500).collect::<String>()
            };
            Err(ServerError::ResponseDeserializationError(format!(
                "Failed to deserialize response of url:{url} response:{abr_response}, error:{e}"
            )))
        }
    }
}
