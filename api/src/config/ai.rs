// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 The Photometoria contributors

use serde::Deserialize;
use std::collections::HashMap;

/// AI provider configuration section.
#[derive(Debug, Clone, Deserialize)]
pub struct AIConfig {
    /// The default provider to use when none is specified.
    #[serde(default)]
    pub default_provider: Option<String>,

    /// The default language for AI-generated tags when not specified per-job.
    #[serde(default)]
    pub default_language: Option<String>,

    /// Named provider configurations.
    #[serde(default)]
    pub providers: HashMap<String, ProviderConfig>,
}

impl AIConfig {
    /// Appends this section's summary to the output buffer.
    pub fn format_summary(&self, out: &mut String) {
        out.push_str(&format!("  [ai]\n"));
        out.push_str(&format!(
            "    default_provider: {}\n",
            self.default_provider.as_deref().unwrap_or("<none>")
        ));
        out.push_str(&format!(
            "    default_language: {}\n",
            self.default_language.as_deref().unwrap_or("<none>")
        ));
        for (name, provider) in &self.providers {
            match provider {
                ProviderConfig::Ollama(ollama) => ollama.format_summary(name, out),
            }
        }
    }
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
            default_language: None,
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

    /// Appends this provider's summary to the output buffer.
    pub fn format_summary(&self, name: &str, out: &mut String) {
        let model_count = self.models.len();
        out.push_str(&format!(
            "    provider '{}': ollama ({} model{})\n",
            name,
            model_count,
            if model_count == 1 { "" } else { "s" }
        ));
        out.push_str(&format!("      base_url: {}\n", self.base_url));
        out.push_str(&format!("      timeout: {}s\n", self.timeout_seconds));
        if !self.devices.is_empty() {
            out.push_str(&format!("      devices: {:?}\n", self.devices));
        }
        for (model_name, model_config) in &self.models {
            model_config.format_summary(model_name, out);
        }
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

    /// Languages this model has been tested or verified to support.
    #[serde(default)]
    pub supported_languages: Vec<String>,
}

impl OllamaModelConfig {
    fn default_supports_vision() -> bool {
        true
    }

    /// Appends this model's summary to the output buffer.
    pub fn format_summary(&self, name: &str, out: &mut String) {
        out.push_str(&format!(
            "      model '{}': {}{}\n",
            name,
            self.ollama_model,
            if self.supports_vision {
                " (vision)"
            } else {
                ""
            }
        ));
        if let Some(ref desc) = self.description {
            out.push_str(&format!("        description: {}\n", desc));
        }
        if let Some(ref template) = self.prompt_template {
            out.push_str(&format!(
                "        prompt_template: {}\n",
                truncate_for_log(template, 50)
            ));
        }
        if !self.supported_languages.is_empty() {
            out.push_str(&format!(
                "        supported_languages: {}\n",
                self.supported_languages.join(", ")
            ));
        }
    }
}

/// Truncates a string for log display, appending "..." if it exceeds `max_len`.
fn truncate_for_log(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_for_log_short_string() {
        assert_eq!(truncate_for_log("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_for_log_exact_length() {
        assert_eq!(truncate_for_log("hello", 5), "hello");
    }

    #[test]
    fn test_truncate_for_log_long_string() {
        assert_eq!(truncate_for_log("hello world", 5), "hello...");
    }

    #[test]
    fn test_truncate_for_log_empty() {
        assert_eq!(truncate_for_log("", 10), "");
    }

    #[test]
    fn test_ai_config_default_language() {
        let toml = r#"
            default_provider = "ollama"
            default_language = "Italian"
        "#;
        let config: AIConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.default_language.as_deref(), Some("Italian"));
    }

    #[test]
    fn test_ai_config_without_default_language() {
        let toml = r#"
            default_provider = "ollama"
        "#;
        let config: AIConfig = toml::from_str(toml).unwrap();
        assert!(config.default_language.is_none());
    }

    #[test]
    fn test_model_config_supported_languages() {
        let toml = r#"
            ollama_model = "qwen3-vl:8b"
            supports_vision = true
            supported_languages = ["English", "Italian", "French"]
        "#;
        let config: OllamaModelConfig = toml::from_str(toml).unwrap();
        assert_eq!(
            config.supported_languages,
            vec!["English", "Italian", "French"]
        );
    }

    #[test]
    fn test_model_config_without_supported_languages() {
        let toml = r#"
            ollama_model = "llava:latest"
            supports_vision = true
        "#;
        let config: OllamaModelConfig = toml::from_str(toml).unwrap();
        assert!(config.supported_languages.is_empty());
    }

    #[test]
    fn test_model_config_empty_supported_languages() {
        let toml = r#"
            ollama_model = "llava:latest"
            supports_vision = true
            supported_languages = []
        "#;
        let config: OllamaModelConfig = toml::from_str(toml).unwrap();
        assert!(config.supported_languages.is_empty());
    }
}
