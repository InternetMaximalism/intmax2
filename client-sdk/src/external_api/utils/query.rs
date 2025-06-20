use std::time::Duration;

use intmax2_interfaces::api::error::ServerError;
use reqwest::{header, Client, Response, Url};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::external_api::utils::retry::with_retry;

/// Timeout for reqwest requests.
/// Because WASM only accepts timeout in the request builder, not the client builder,
/// we set a timeout of 30 seconds for each request.
pub const REQWEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Maximum response body size for logging (in characters)
const MAX_RESPONSE_LOG_SIZE: usize = 500;

#[derive(Debug, Deserialize)]
struct ErrorResponse {
    error: String,
    #[serde(default)]
    message: Option<String>,
}

/// Builds a new HTTP client with default configuration
pub fn build_client() -> Client {
    reqwest::Client::builder()
        .build()
        .expect("Failed to build reqwest client")
}

/// Makes a POST request without authentication
pub async fn post_request<B: Serialize, R: DeserializeOwned>(
    client: &Client,
    base_url: &str,
    endpoint: &str,
    body: Option<&B>,
) -> Result<R, ServerError> {
    post_request_with_bearer_token(client, base_url, endpoint, None, body).await
}

/// Makes a POST request with optional bearer token authentication
pub async fn post_request_with_bearer_token<B: Serialize, R: DeserializeOwned>(
    client: &Client,
    base_url: &str,
    endpoint: &str,
    bearer_token: Option<&str>,
    body: Option<&B>,
) -> Result<R, ServerError> {
    let url = build_url(base_url, endpoint)?;

    let mut request_builder = client.post(url.clone()).timeout(REQWEST_TIMEOUT);

    if let Some(token) = bearer_token {
        let auth_header = if token.starts_with("Bearer ") {
            token.to_string()
        } else {
            format!("Bearer {token}")
        };
        request_builder = request_builder.header(header::AUTHORIZATION, auth_header);
    }

    if let Some(body) = body {
        request_builder = request_builder.json(body);
    }

    let body_info = serialize_body_for_logging(body)?;
    log::debug!(
        "POST request url: {} body size: {} bytes",
        url,
        body_info.size
    );

    let response = execute_request_with_retry(request_builder).await?;
    handle_response(response, url, &body_info.content).await
}

/// Makes a GET request with optional query parameters
pub async fn get_request<Q, R>(
    client: &Client,
    base_url: &str,
    endpoint: &str,
    query: Option<&Q>, // Changed to &Q to avoid unnecessary cloning
) -> Result<R, ServerError>
where
    Q: Serialize,
    R: DeserializeOwned,
{
    let mut url = build_url(base_url, endpoint)?;

    let query_info = if let Some(query) = query {
        let query_str = serde_qs::to_string(query)
            .map_err(|e| ServerError::SerializeError(format!("Failed to serialize query: {e}")))?;
        url.set_query(Some(&query_str));
        Some(query_str)
    } else {
        None
    };

    let request_builder = client.get(url.clone()).timeout(REQWEST_TIMEOUT);

    log::debug!("GET request url: {url}");

    let response = execute_request_with_retry(request_builder).await?;
    handle_response(response, url, &query_info).await
}

/// Information about serialized request body for logging
#[derive(Debug)]
struct BodyInfo {
    content: Option<String>,
    size: usize,
}

/// Serializes request body for logging purposes
fn serialize_body_for_logging<B: Serialize>(body: Option<&B>) -> Result<BodyInfo, ServerError> {
    if let Some(body) = body {
        let body_str = serde_json::to_string(body)
            .map_err(|e| ServerError::SerializeError(format!("Failed to serialize body: {e}")))?;
        let size = body_str.len();
        Ok(BodyInfo {
            content: Some(body_str),
            size,
        })
    } else {
        Ok(BodyInfo {
            content: None,
            size: 0,
        })
    }
}

/// Executes a request with retry logic
async fn execute_request_with_retry(
    request_builder: reqwest::RequestBuilder,
) -> Result<Response, ServerError> {
    with_retry(|| async { request_builder.try_clone().unwrap().send().await })
        .await
        .map_err(|e| ServerError::NetworkError(e.to_string()))
}

/// Handles HTTP response, including error cases and deserialization
async fn handle_response<R: DeserializeOwned>(
    response: Response,
    url: Url,
    request_info: &Option<String>,
) -> Result<R, ServerError> {
    let status = response.status();

    if !status.is_success() {
        return handle_error_response(response, url, request_info).await;
    }

    let response_text = response.text().await.map_err(|e| {
        ServerError::ResponseDeserializationError(format!("Failed to read response: {e}"))
    })?;

    deserialize_response(&response_text, url)
}

/// Handles error responses from the server
async fn handle_error_response<R>(
    response: Response,
    url: Url,
    request_info: &Option<String>,
) -> Result<R, ServerError> {
    let status = response.status();
    let error_text = response
        .text()
        .await
        .unwrap_or_else(|_| "Failed to read error response".to_string());

    let error_message = parse_error_message(&error_text);
    let request_debug_info = format_request_debug_info(request_info);
    let sanitized_url = sanitize_url(url);

    Err(ServerError::ResponseError(format!(
      "Request to url:{sanitized_url} failed with status:{status}, error:{error_message}{request_debug_info}",
  )))
}

/// Parses error message from response text
fn parse_error_message(error_text: &str) -> String {
    match serde_json::from_str::<ErrorResponse>(error_text) {
        Ok(error_resp) => error_resp.message.unwrap_or(error_resp.error),
        Err(_) => error_text.to_string(),
    }
}

/// Formats request information for debug logging
fn format_request_debug_info(request_info: &Option<String>) -> String {
    if log::log_enabled!(log::Level::Debug) {
        format!(", request:{}", request_info.as_deref().unwrap_or(""))
    } else {
        String::new()
    }
}

/// Deserializes response text to the expected type
fn deserialize_response<R: DeserializeOwned>(
    response_text: &str,
    url: Url,
) -> Result<R, ServerError> {
    match serde_json::from_str::<R>(response_text) {
        Ok(result) => Ok(result),
        Err(e) => {
            let truncated_response = if log::log_enabled!(log::Level::Debug) {
                response_text.to_string()
            } else {
                response_text
                    .chars()
                    .take(MAX_RESPONSE_LOG_SIZE)
                    .collect::<String>()
            };

            Err(ServerError::ResponseDeserializationError(format!(
              "Failed to deserialize response from url:{url} response:{truncated_response}, error:{e}"
          )))
        }
    }
}

/*
* Utility functions
*/

/// Builds a complete URL from base URL and endpoint
fn build_url(base_url: &str, endpoint: &str) -> Result<Url, ServerError> {
    let base_url = Url::parse(base_url)
        .map_err(|e| ServerError::MalformedUrl(format!("Invalid URL: {base_url}, error: {e}")))?;
    let normalized_endpoint = if endpoint.starts_with('/') {
        endpoint.to_string()
    } else {
        format!("/{endpoint}")
    };
    let url = Url::parse(&format!("{base_url}{normalized_endpoint}")).map_err(|e| {
        ServerError::MalformedUrl(format!(
            "Invalid URL: {base_url}{normalized_endpoint}, error: {e}"
        ))
    })?;
    Ok(url)
}

/// Sanitizes URL by removing query parameters for logging
fn sanitize_url(url: Url) -> Url {
    let mut sanitized_url = url.clone();
    sanitized_url.set_query(None);
    sanitized_url
}
