mod byte_size;

use crate::config::byte_size::ByteSize;
use serde::Deserialize;
use std::fs;
use std::path::Path;

/// Server configuration section.
#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    /// Bind address for the HTTP server.
    pub host: String,
    /// TCP port to listen on.
    pub port: u16,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 3000,
        }
    }
}

/// Storage configuration section
#[derive(Debug, Clone, Deserialize)]
pub struct StorageConfig {
    pub path: String,
    pub max_size: ByteSize,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            path: "/var/photometoria/storage".to_string(),
            max_size: "10GiB".parse().unwrap(),
        }
    }
}

/// Upload configuration section
#[derive(Debug, Clone, Deserialize)]
pub struct UploadConfig {
    pub max_photos_per_request: usize,
    pub max_photo_size: ByteSize,
}

impl Default for UploadConfig {
    fn default() -> Self {
        Self {
            max_photos_per_request: 100,
            max_photo_size: "20MB".parse().unwrap(),
        }
    }
}

/// Application configuration.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Config {
    /// Server binding and network configuration.
    pub server: ServerConfig,
    pub storage: StorageConfig,
    pub upload: UploadConfig,
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
            tracing::warn!(
                "Configuration file '{}' not found, using default values",
                path_str
            );
            return Ok(Config::default());
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

    tracing::info!("Loaded configuration from '{}'", path_str);
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
}
