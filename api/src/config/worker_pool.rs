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
}

impl WorkerPoolConfig {
    fn default_min_photos_before_swap() -> usize {
        10
    }

    fn default_max_time_before_swap() -> String {
        "120s".to_string()
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

        Ok(())
    }
}

impl Default for WorkerPoolConfig {
    fn default() -> Self {
        Self {
            min_photos_before_swap: Self::default_min_photos_before_swap(),
            max_time_before_swap: Self::default_max_time_before_swap(),
        }
    }
}

/// Parses a duration string like "60s" or "2m" into seconds.
pub fn parse_duration(s: &str) -> Option<std::time::Duration> {
    let s = s.trim();
    if let Some(secs) = s.strip_suffix('s') {
        secs.parse::<u64>().ok().map(std::time::Duration::from_secs)
    } else if let Some(mins) = s.strip_suffix('m') {
        mins.parse::<u64>()
            .ok()
            .map(|m| std::time::Duration::from_secs(m * 60))
    } else {
        None
    }
}
