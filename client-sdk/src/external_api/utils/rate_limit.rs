use std::{f64, future::Future, sync::Arc};

use tokio::{
    sync::Mutex,
    time::{Duration, Instant},
};

/// Simple token bucket rate limiter that works on native and WASM targets.
#[derive(Debug)]
pub struct RequestRateLimiter {
    state: Mutex<LimiterState>,
    refill_rate_per_sec: f64,
    capacity: f64,
}

#[derive(Debug)]
struct LimiterState {
    tokens: f64,
    last_refill: Instant,
}

impl RequestRateLimiter {
    /// Create a rate limiter with a maximum burst (`capacity`) and steady refill rate (`permits_per_second`).
    ///
    /// # Panics
    /// Panics if either parameter is zero.
    pub fn new(permits_per_second: u32, capacity: u32) -> Self {
        assert!(
            permits_per_second > 0,
            "permits_per_second must be positive"
        );
        assert!(capacity > 0, "capacity must be positive");

        let refill_rate_per_sec = permits_per_second as f64;
        let capacity = capacity as f64;

        RequestRateLimiter {
            state: Mutex::new(LimiterState {
                tokens: capacity,
                last_refill: Instant::now(),
            }),
            refill_rate_per_sec,
            capacity,
        }
    }

    /// Acquire a single permit before proceeding.
    pub async fn acquire(&self) {
        self.acquire_permits(1).await;
    }

    /// Acquire `permits` units before proceeding.
    pub async fn acquire_permits(&self, permits: u32) {
        assert!(permits > 0, "permits must be positive");
        let permits = permits as f64;

        loop {
            let mut state = self.state.lock().await;
            let now = Instant::now();
            let elapsed = now - state.last_refill;

            if elapsed > Duration::from_secs(0) {
                let refill_amount = elapsed.as_secs_f64() * self.refill_rate_per_sec;
                state.tokens = (state.tokens + refill_amount).min(self.capacity);
                state.last_refill = now;
            }

            if state.tokens >= permits {
                state.tokens -= permits;
                break;
            }

            let deficit = permits - state.tokens;
            let wait_seconds = deficit / self.refill_rate_per_sec;
            let sleep_duration = Duration::from_secs_f64(wait_seconds.clamp(0.0, f64::MAX));

            // Drop the lock before awaiting.
            drop(state);
            tokio::time::sleep(sleep_duration).await;
        }
    }

    /// Acquire permits, then execute the provided future.
    pub async fn run<F, Fut, T>(&self, permits: u32, op: F) -> T
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = T>,
    {
        self.acquire_permits(permits).await;
        op().await
    }
}

pub fn limiter_from_env(
    permits_var: &str,
    burst_var: &str,
    default_rps: u32,
    default_burst_multiplier: u32,
) -> Arc<RequestRateLimiter> {
    let permits_per_second = std::env::var(permits_var)
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(default_rps);

    let default_capacity = permits_per_second.saturating_mul(default_burst_multiplier);

    let capacity = std::env::var(burst_var)
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(default_capacity.max(permits_per_second));

    Arc::new(RequestRateLimiter::new(permits_per_second, capacity))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn waits_until_tokens_refilled() {
        let limiter = RequestRateLimiter::new(5, 5);

        // Consume entire bucket instantly.
        limiter.acquire_permits(5).await;

        let before = Instant::now();
        limiter.acquire().await;
        let waited = Instant::now() - before;

        assert!(
            waited >= Duration::from_millis(180),
            "expected to wait for refill, but only waited {waited:?}",
        );
    }
}
