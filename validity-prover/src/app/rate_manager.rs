use chrono::Utc;
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{sync::Mutex, time::timeout};

#[derive(Debug, thiserror::Error)]
pub enum RateManagerError {
    #[error("Timeout: {0}")]
    Timeout(String),
}

#[derive(Debug, Clone)]
pub struct RateManager {
    pub window: Duration,
    pub timeout: Duration,

    // counts with cleanup
    pub counts: Arc<Mutex<HashMap<String, Vec<Instant>>>>,

    // last timestamps
    pub last_timestamps: Arc<Mutex<HashMap<String, u64>>>,

    // stop flags
    pub stop_flags: Arc<Mutex<HashMap<String, bool>>>,
}

impl RateManager {
    pub fn new(window: Duration, timeout: Duration) -> Self {
        Self {
            window,
            timeout,
            counts: Arc::new(Mutex::new(HashMap::new())),
            last_timestamps: Arc::new(Mutex::new(HashMap::new())),
            stop_flags: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn reset(&self) -> Result<(), RateManagerError> {
        let mut counts = timeout(self.timeout, self.counts.lock())
            .await
            .map_err(|_| RateManagerError::Timeout("Timeout while resetting keys".to_string()))?;
        counts.clear();
        drop(counts);

        let mut stop_flags = timeout(self.timeout, self.stop_flags.lock())
            .await
            .map_err(|_| RateManagerError::Timeout("Timeout while resetting keys".to_string()))?;
        stop_flags.clear();

        let mut last_timestamps = timeout(self.timeout, self.last_timestamps.lock())
            .await
            .map_err(|_| RateManagerError::Timeout("Timeout while resetting keys".to_string()))?;
        last_timestamps.clear();
        Ok(())
    }

    pub async fn add(&self, key: &str) -> Result<(), RateManagerError> {
        self.cleanup().await?;

        let mut counts = timeout(self.timeout, self.counts.lock())
            .await
            .map_err(|_| RateManagerError::Timeout("Timeout while adding key".to_string()))?;
        counts
            .entry(key.to_string())
            .or_insert_with(Vec::new)
            .push(Instant::now());
        drop(counts);

        self.emit_heartbeat(key).await?;
        Ok(())
    }

    pub async fn count(&self, key: &str) -> Result<usize, RateManagerError> {
        let counts = timeout(self.timeout, self.counts.lock())
            .await
            .map_err(|_| RateManagerError::Timeout("Timeout while counting keys".to_string()))?;
        let count = counts
            .get(key)
            .map(|timestamps| {
                timestamps
                    .iter()
                    .filter(|&&timestamp| timestamp.elapsed() <= self.window)
                    .count()
            })
            .unwrap_or(0);
        Ok(count)
    }

    /// Emit a heartbeat for thread health check
    pub async fn emit_heartbeat(&self, key: &str) -> Result<(), RateManagerError> {
        let mut last_timestamps = timeout(self.timeout, self.last_timestamps.lock())
            .await
            .map_err(|_| {
                RateManagerError::Timeout("Timeout while emitting heartbeat".to_string())
            })?;
        last_timestamps.insert(key.to_string(), Utc::now().timestamp() as u64);
        Ok(())
    }

    pub async fn get_last_heartbeat(&self, key: &str) -> Result<Option<u64>, RateManagerError> {
        let last_timestamps = timeout(self.timeout, self.last_timestamps.lock())
            .await
            .map_err(|_| {
                RateManagerError::Timeout("Timeout while getting last heartbeat".to_string())
            })?;
        let last_timestamp = last_timestamps.get(key).cloned();
        Ok(last_timestamp)
    }

    pub async fn set_stop_flag(&self, key: &str, flag: bool) -> Result<(), RateManagerError> {
        let mut stop_flags = timeout(self.timeout, self.stop_flags.lock())
            .await
            .map_err(|_| {
                RateManagerError::Timeout("Timeout while setting stop flag".to_string())
            })?;
        stop_flags.insert(key.to_string(), flag);
        Ok(())
    }

    pub async fn get_stop_flag(&self, key: &str) -> Result<bool, RateManagerError> {
        let stop_flags = timeout(self.timeout, self.stop_flags.lock())
            .await
            .map_err(|_| {
                RateManagerError::Timeout("Timeout while getting stop flag".to_string())
            })?;
        Ok(stop_flags.get(key).cloned().unwrap_or(false))
    }

    async fn cleanup(&self) -> Result<(), RateManagerError> {
        let mut counts = timeout(self.timeout, self.counts.lock())
            .await
            .map_err(|_| RateManagerError::Timeout("Timeout while cleaning up keys".to_string()))?;
        for timestamps in counts.values_mut() {
            timestamps.retain(|&timestamp| timestamp.elapsed() <= self.window);
        }
        Ok(())
    }
}
