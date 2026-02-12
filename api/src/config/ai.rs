use serde::Deserialize;
use std::collections::HashMap;

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
}

/// Configuration for an Ollama model.
#[derive(Debug, Clone, Deserialize)]
pub struct OllamaModelConfig {
    /// The actual Ollama model name (e.g., "qwen3-vl:8b").
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
