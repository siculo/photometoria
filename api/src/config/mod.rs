pub mod byte_size;

pub use byte_size::ByteSize;
use serde::Deserialize;
use std::collections::HashMap;
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
            path: "./storage".to_string(),
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

/// AI provider configuration section.
#[derive(Debug, Clone, Deserialize)]
pub struct AIConfig {
    /// The default provider to use when none is specified.
    #[serde(default)]
    pub default_provider: Option<String>,

    /// Named provider configurations.
    #[serde(default)]
    pub providers: HashMap<String, ProviderConfig>,
}

impl Default for AIConfig {
    fn default() -> Self {
        let default_provider_name = "ollama";
        let mut providers = HashMap::default();
        providers.insert(
            default_provider_name.to_string(),
            ProviderConfig::Ollama(Default::default()),
        );
        Self {
            default_provider: Some(default_provider_name.to_string()),
            providers,
        }
    }
}

/// Configuration for a single AI provider.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum ProviderConfig {
    /// Ollama provider configuration.
    #[serde(rename = "ollama")]
    Ollama(OllamaProviderConfig),
}

/// Ollama-specific provider configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct OllamaProviderConfig {
    /// Base URL for the Ollama API.
    #[serde(default = "OllamaProviderConfig::default_base_url")]
    pub base_url: String,

    /// Request timeout in seconds.
    #[serde(default = "OllamaProviderConfig::default_timeout_seconds")]
    pub timeout_seconds: u64,

    /// GPU device IDs to use (empty means auto-detect).
    #[serde(default)]
    pub devices: Vec<u32>,

    /// Maximum number of concurrent workers.
    #[serde(default = "OllamaProviderConfig::default_max_workers")]
    pub max_workers: usize,

    /// Model configurations for this provider.
    #[serde(default)]
    pub models: HashMap<String, OllamaModelConfig>,
}

impl Default for OllamaProviderConfig {
    fn default() -> Self {
        Self {
            base_url: Self::default_base_url(),
            timeout_seconds: Self::default_timeout_seconds(),
            devices: Vec::new(),
            max_workers: Self::default_max_workers(),
            models: HashMap::new(),
        }
    }
}

impl OllamaProviderConfig {
    fn default_base_url() -> String {
        "http://localhost:11434".to_string()
    }

    fn default_timeout_seconds() -> u64 {
        120
    }

    fn default_max_workers() -> usize {
        2
    }
}

/// Configuration for an Ollama model.
#[derive(Debug, Clone, Deserialize)]
pub struct OllamaModelConfig {
    /// The actual Ollama model name (e.g., "qwen2-vl:8b").
    pub ollama_model: String,

    /// The prompt template to use for image analysis.
    #[serde(default)]
    pub prompt_template: Option<String>,

    /// Human-readable description of the model.
    #[serde(default)]
    pub description: Option<String>,

    /// Whether this model supports vision/image analysis.
    #[serde(default = "OllamaModelConfig::default_supports_vision")]
    pub supports_vision: bool,
}

impl OllamaModelConfig {
    fn default_supports_vision() -> bool {
        true
    }
}

/// Application configuration.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Config {
    /// Server binding and network configuration.
    pub server: ServerConfig,
    pub storage: StorageConfig,
    pub upload: UploadConfig,
    /// AI provider configuration.
    #[serde(default)]
    pub ai: AIConfig,
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
