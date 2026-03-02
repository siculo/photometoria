// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 The Photometoria contributors

//! AI provider registry.
//!
//! The registry manages multiple AI providers and provides a unified
//! interface for accessing them by name or through a default provider.

use std::collections::HashMap;
use std::sync::Arc;

use crate::config::{AIConfig, ProviderConfig};

use super::error::{AIProviderError, AIProviderResult};
use super::ollama::create_ollama_provider;
use super::provider::AIProvider;

/// Registry for AI providers.
///
/// This struct manages a collection of AI providers and provides
/// methods to access them by name or through a configured default.
pub struct ProviderRegistry {
    /// Map of provider name to provider instance.
    providers: HashMap<String, Arc<dyn AIProvider>>,

    /// The name of the default provider (if configured).
    default_provider_name: Option<String>,
}

impl ProviderRegistry {
    /// Creates a new empty registry.
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
            default_provider_name: None,
        }
    }

    /// Creates a registry from configuration.
    ///
    /// This method instantiates all configured providers and registers them.
    pub fn from_config(config: &AIConfig) -> AIProviderResult<Self> {
        let mut registry = Self::new();

        // Register each configured provider
        for (name, provider_config) in &config.providers {
            let provider = match provider_config {
                ProviderConfig::Ollama(ollama_config) => {
                    create_ollama_provider(name.clone(), ollama_config)
                }
            };

            registry.register(name.clone(), provider);
            tracing::info!("Registered AI provider: {}", name);
        }

        // Set default provider if configured
        if let Some(default_name) = &config.default_provider {
            if registry.providers.contains_key(default_name) {
                registry.default_provider_name = Some(default_name.clone());
                tracing::info!("Default AI provider: {}", default_name);
            } else {
                return Err(AIProviderError::ConfigurationError {
                    provider: default_name.clone(),
                    message: format!(
                        "Default provider '{}' is not configured in [ai.providers]",
                        default_name
                    ),
                });
            }
        } else if registry.providers.len() == 1 {
            // If only one provider, make it the default
            let name = registry.providers.keys().next().unwrap().clone();
            registry.default_provider_name = Some(name.clone());
            tracing::info!("Auto-selected default AI provider: {}", name);
        }

        Ok(registry)
    }

    /// Registers a provider with the given name.
    ///
    /// If a provider with the same name already exists, it will be replaced.
    pub fn register(&mut self, name: impl Into<String>, provider: Arc<dyn AIProvider>) {
        self.providers.insert(name.into(), provider);
    }

    /// Sets the default provider by name.
    ///
    /// # Errors
    ///
    /// Returns an error if no provider with the given name is registered.
    pub fn set_default(&mut self, name: impl Into<String>) -> AIProviderResult<()> {
        let name = name.into();
        if self.providers.contains_key(&name) {
            self.default_provider_name = Some(name);
            Ok(())
        } else {
            Err(AIProviderError::ConfigurationError {
                provider: name.clone(),
                message: format!("Provider '{}' is not registered", name),
            })
        }
    }

    /// Returns the names of available providers.
    pub fn get_names(&self) -> Vec<String> {
        self.providers.keys().cloned().collect()
    }

    /// Gets a provider by name.
    ///
    /// # Errors
    ///
    /// Returns an error if no provider with the given name is registered.
    pub fn get(&self, name: &str) -> AIProviderResult<Arc<dyn AIProvider>> {
        self.providers
            .get(name)
            .cloned()
            .ok_or_else(|| AIProviderError::ConfigurationError {
                provider: name.to_string(),
                message: format!("Provider '{}' is not registered", name),
            })
    }

    /// Gets the default provider.
    ///
    /// # Errors
    ///
    /// Returns an error if no default provider is configured or
    /// if the configured default is not registered.
    pub fn default_provider(&self) -> AIProviderResult<Arc<dyn AIProvider>> {
        match &self.default_provider_name {
            Some(name) => self.get(name),
            None => Err(AIProviderError::ConfigurationError {
                provider: "default".to_string(),
                message: "No default AI provider configured".to_string(),
            }),
        }
    }

    /// Returns the name of the default provider, if configured.
    pub fn default_provider_name(&self) -> Option<&str> {
        self.default_provider_name.as_deref()
    }

    /// Returns all registered provider names.
    pub fn provider_names(&self) -> Vec<&str> {
        self.providers.keys().map(|s| s.as_str()).collect()
    }

    /// Returns the number of registered providers.
    pub fn len(&self) -> usize {
        self.providers.len()
    }

    /// Returns true if no providers are registered.
    /// Returns true if the given model ID is configured in the default provider.
    ///
    /// Does not make a network call — validates against the static configuration.
    pub fn is_model_configured(&self, model_id: &str) -> bool {
        self.default_provider()
            .map(|p| p.configured_model_ids().contains(&model_id.to_string()))
            .unwrap_or(false)
    }

    pub fn is_empty(&self) -> bool {
        self.providers.is_empty()
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{OllamaModelConfig, OllamaProviderConfig};
    use std::collections::HashMap;

    fn create_test_config() -> AIConfig {
        let mut providers = HashMap::new();

        let mut models = HashMap::new();
        models.insert(
            "test-model".to_string(),
            OllamaModelConfig {
                ollama_model: "llava:latest".to_string(),
                prompt_template: None,
                description: None,
                supports_vision: true,
            },
        );

        providers.insert(
            "ollama".to_string(),
            ProviderConfig::Ollama(OllamaProviderConfig {
                base_url: "http://localhost:11434".to_string(),
                timeout_seconds: 30,
                devices: vec![],
                models,
            }),
        );

        AIConfig {
            default_provider: Some("ollama".to_string()),
            providers,
        }
    }

    #[test]
    fn test_registry_from_config() {
        let config = create_test_config();
        let registry = ProviderRegistry::from_config(&config).unwrap();

        assert_eq!(registry.len(), 1);
        assert_eq!(registry.default_provider_name(), Some("ollama"));
        assert!(registry.get("ollama").is_ok());
    }

    #[test]
    fn test_registry_get_nonexistent() {
        let registry = ProviderRegistry::new();
        let result = registry.get("nonexistent");

        assert!(result.is_err());
        if let Err(AIProviderError::ConfigurationError { provider, .. }) = result {
            assert_eq!(provider, "nonexistent");
        } else {
            panic!("Expected ConfigurationError");
        }
    }

    #[test]
    fn test_registry_no_default() {
        let registry = ProviderRegistry::new();
        let result = registry.default_provider();

        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_default_provider() {
        let config = AIConfig {
            default_provider: Some("nonexistent".to_string()),
            providers: HashMap::new(),
        };

        let result = ProviderRegistry::from_config(&config);
        assert!(result.is_err());
    }

    #[test]
    fn test_auto_select_default() {
        let mut providers = HashMap::new();
        providers.insert(
            "only-provider".to_string(),
            ProviderConfig::Ollama(OllamaProviderConfig::default()),
        );

        let config = AIConfig {
            default_provider: None, // Not explicitly set
            providers,
        };

        let registry = ProviderRegistry::from_config(&config).unwrap();

        // Should auto-select the only provider as default
        assert_eq!(registry.default_provider_name(), Some("only-provider"));
    }
}
