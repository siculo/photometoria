// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 The Photometoria contributors

use serde::Deserialize;

/// Worker pool configuration section.
#[derive(Debug, Clone, Deserialize)]
pub struct WorkerPoolConfig {
    /// Minimum photos to process before allowing model swap.
    #[serde(default = "WorkerPoolConfig::default_min_photos_before_swap")]
    pub min_photos_before_swap: usize,

    /// Maximum time with same model before forcing swap (e.g., "120s", "2m").
    #[serde(default = "WorkerPoolConfig::default_max_time_before_swap")]
    pub max_time_before_swap: String,

    /// How long a worker sleeps when the photo buffer is empty (e.g., "500ms", "1s").
    #[serde(default = "WorkerPoolConfig::default_worker_idle_sleep")]
    pub worker_idle_sleep: String,

    /// How often the discovery loop polls for new queued jobs (e.g., "5s", "1m").
    #[serde(default = "WorkerPoolConfig::default_discovery_poll_interval")]
    pub discovery_poll_interval: String,
}

impl WorkerPoolConfig {
    fn default_min_photos_before_swap() -> usize {
        10
    }

    fn default_max_time_before_swap() -> String {
        "120s".to_string()
    }

    fn default_worker_idle_sleep() -> String {
        "500ms".to_string()
    }

    fn default_discovery_poll_interval() -> String {
        "5s".to_string()
    }

    /// Validates the worker pool configuration.
    /// Returns a descriptive error message if invalid.
    pub fn validate(&self) -> Result<(), String> {
        if self.min_photos_before_swap < 1 {
            return Err("worker_pool.min_photos_before_swap must be >= 1".to_string());
        }

        parse_duration(&self.max_time_before_swap).ok_or_else(|| {
            format!(
                "worker_pool.max_time_before_swap '{}' is invalid: use formats like '120s' or '2m'",
                self.max_time_before_swap
            )
        })?;

        parse_duration(&self.worker_idle_sleep).ok_or_else(|| {
            format!(
                "worker_pool.worker_idle_sleep '{}' is invalid: use formats like '500ms', '1s' or '1m'",
                self.worker_idle_sleep
            )
        })?;

        parse_duration(&self.discovery_poll_interval).ok_or_else(|| {
            format!(
                "worker_pool.discovery_poll_interval '{}' is invalid: use formats like '5s' or '1m'",
                self.discovery_poll_interval
            )
        })?;

        Ok(())
    }
}

impl Default for WorkerPoolConfig {
    fn default() -> Self {
        Self {
            min_photos_before_swap: Self::default_min_photos_before_swap(),
            max_time_before_swap: Self::default_max_time_before_swap(),
            worker_idle_sleep: Self::default_worker_idle_sleep(),
            discovery_poll_interval: Self::default_discovery_poll_interval(),
        }
    }
}

/// Parses a duration string like "500ms", "60s" or "2m".
pub fn parse_duration(s: &str) -> Option<std::time::Duration> {
    let s = s.trim();
    if let Some(ms) = s.strip_suffix("ms") {
        ms.parse::<u64>().ok().map(std::time::Duration::from_millis)
    } else if let Some(secs) = s.strip_suffix('s') {
        secs.parse::<u64>().ok().map(std::time::Duration::from_secs)
    } else if let Some(mins) = s.strip_suffix('m') {
        mins.parse::<u64>()
            .ok()
            .map(|m| std::time::Duration::from_secs(m * 60))
    } else {
        None
    }
}
