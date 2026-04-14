// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 The Photometoria contributors

pub mod ai;
pub mod byte_size;
pub mod server;
pub mod storage;
pub mod task;
pub mod upload;
pub mod worker_pool;

pub use ai::{AIConfig, OllamaModelConfig, OllamaProviderConfig, ProviderConfig};
pub use byte_size::ByteSize;
pub use server::ServerConfig;
pub use storage::StorageConfig;
pub use task::TaskConfig;
pub use upload::UploadConfig;
pub use worker_pool::WorkerPoolConfig;

use std::fs;
use std::path::Path;

/// Application configuration.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct Config {
    /// Server binding and network configuration.
    pub server: ServerConfig,
    pub storage: StorageConfig,
    pub upload: UploadConfig,
    /// Task-related configuration.
    #[serde(default)]
    pub task: TaskConfig,
    /// AI provider configuration.
    #[serde(default)]
    pub ai: AIConfig,
    /// Worker pool configuration.
    #[serde(default)]
    pub worker_pool: WorkerPoolConfig,
}

impl Config {
    /// Returns the server address as a string suitable for binding.
    pub fn server_addr(&self) -> String {
        format!("{}:{}", self.server.host, self.server.port)
    }

    /// Returns the storage max size in bytes
    pub fn storage_max_size(&self) -> u64 {
        self.storage.max_size.0
    }

    /// Logs the effective configuration at `info` level in a human-readable,
    /// section-based format as a single log entry.
    pub fn log_summary(&self) {
        let mut out = String::from("Configuration:\n");
        self.server.format_summary(&mut out);
        self.storage.format_summary(&mut out);
        self.upload.format_summary(&mut out);
        self.task.format_summary(&mut out);
        self.ai.format_summary(&mut out);
        self.worker_pool.format_summary(&mut out);
        tracing::info!("{}", out.trim_end());
    }
}

/// Loads configuration from file.
///
/// In debug builds, returns default configuration with a warning if file is missing.
/// In release builds, returns an error if file is missing.
pub fn load_config(config_path: &Path) -> Result<Config, ConfigError> {
    let path_str = config_path.display().to_string();

    if !config_path.exists() {
        #[cfg(debug_assertions)]
        {
            let config = Config::default();
            tracing::warn!(
                "Configuration file '{}' not found, using default values",
                path_str
            );
            config.log_summary();
            return Ok(config);
        }

        #[cfg(not(debug_assertions))]
        {
            return Err(ConfigError::FileNotFound(path_str));
        }
    }

    let content = fs::read_to_string(config_path).map_err(|e| ConfigError::ReadError {
        path: path_str.clone(),
        source: e,
    })?;
    let config: Config = toml::from_str(&content).map_err(|e| ConfigError::ParseError {
        path: path_str.clone(),
        source: e,
    })?;

    config
        .worker_pool
        .validate()
        .map_err(ConfigError::ValidationError)?;

    tracing::info!("Loaded configuration from '{}'", path_str);
    config.log_summary();

    Ok(config)
}

/// Configuration loading errors.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Configuration file '{0}' not found")]
    #[allow(dead_code)]
    FileNotFound(String),

    #[error("Failed to read configuration file '{path}': {source}")]
    ReadError {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to parse configuration file '{path}': {source}")]
    ParseError {
        path: String,
        #[source]
        source: toml::de::Error,
    },

    #[error("Invalid configuration: {0}")]
    ValidationError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_summary_does_not_panic() {
        let config = Config::default();
        config.log_summary();
    }
}
