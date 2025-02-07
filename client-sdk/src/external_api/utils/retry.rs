use std::{fmt::Debug, future::Future, time::Duration};

use log::warn;

use crate::external_api::utils::time::sleep_for;

const MAX_RETRIES: u32 = 5;
const INITIAL_DELAY: u64 = 1000;

#[derive(Debug, Clone, Copy)]
pub struct RetryConfig {
    pub max_retries: u32,
    pub initial_delay: u64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        RetryConfig {
            max_retries: MAX_RETRIES,
            initial_delay: INITIAL_DELAY,
        }
    }
}

pub async fn retry_if<'a, T, E, F, Fut, Cond>(
    condition: Cond,
    f: F,
    config: RetryConfig,
) -> Result<T, E>
where
    E: Debug,
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T, E>> + 'a,
    Cond: Fn(&E) -> bool,
{
    let mut retries = 0;
    let mut delay = Duration::from_millis(config.initial_delay);

    loop {
        match f().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                if !condition(&e) || retries >= config.max_retries {
                    return Err(e);
                }
                warn!(
                    "Attempt {} failed: {:?}. Retrying in {:?}...",
                    retries + 1,
                    e,
                    delay
                );
                sleep_for(delay.as_secs()).await;
                retries += 1;
                delay *= 2; // Exponential backoff
            }
        }
    }
}

pub async fn with_retry<'a, T, E, F, Fut>(f: F) -> Result<T, E>
where
    E: std::error::Error,
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T, E>> + 'a,
{
    let mut retries = 0;
    let mut delay = Duration::from_millis(INITIAL_DELAY);

    loop {
        match f().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                if retries >= MAX_RETRIES {
                    return Err(e);
                }
                warn!(
                    "Attempt {} failed: {}. Retrying in {:?}...",
                    retries + 1,
                    e.to_string(),
                    delay
                );
                sleep_for(delay.as_secs()).await;
                retries += 1;
                delay *= 2; // Exponential backoff
            }
        }
    }
}
