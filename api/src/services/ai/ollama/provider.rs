// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 The Photometoria contributors

//! Ollama AI provider implementation.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use reqwest::Client;
use tracing::debug;
use crate::config::OllamaModelConfig;

use super::super::error::{AIProviderError, AIProviderResult};
use super::super::provider::{
    AIProvider, AnalyzeImageRequest, AnalyzeImageResponse, HealthStatus, ModelInfo, TokenUsage,
};
use super::types::{OllamaGenerateRequest, OllamaGenerateResponse, OllamaTagsResponse};

/// Ollama AI provider implementation.
///
/// This provider communicates with a local or remote Ollama instance
/// to perform image analysis using vision models.
pub struct OllamaProvider {
    /// Provider name for identification.
    name: String,

    /// HTTP client for API requests.
    client: Client,

    /// Base URL for the Ollama API.
    base_url: String,

    /// Request timeout.
    timeout: Duration,

    /// Configured models with their settings.
    configured_models: HashMap<String, OllamaModelConfig>,
}

impl OllamaProvider {
    /// Creates a new Ollama provider instance.
    ///
    /// # Arguments
    ///
    /// * `name` - Unique name for this provider instance.
    /// * `base_url` - Base URL for the Ollama API (e.g., "http://localhost:11434").
    /// * `timeout_seconds` - Request timeout in seconds.
    /// * `models` - Map of model configurations.
    pub fn new(
        name: impl Into<String>,
        base_url: impl Into<String>,
        timeout_seconds: u64,
        models: HashMap<String, OllamaModelConfig>,
    ) -> Self {
        let timeout = Duration::from_secs(timeout_seconds);

        let client = Client::builder()
            .timeout(timeout)
            .build()
            .expect("Failed to create HTTP client");

        Self {
            name: name.into(),
            client,
            base_url: base_url.into(),
            timeout,
            configured_models: models,
        }
    }

    /// Returns the full URL for an API endpoint.
    fn api_url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    /// Gets the Ollama model name for a configured model ID.
    fn get_ollama_model_name(&self, model_id: &str) -> Option<&str> {
        self.configured_models
            .get(model_id)
            .map(|config| config.ollama_model.as_str())
    }

    /// Gets the prompt template for a model, or returns the default prompt.
    fn get_prompt_for_model(&self, model_id: &str, default_prompt: &str) -> String {
        self.configured_models
            .get(model_id)
            .and_then(|config| config.prompt_template.as_ref())
            .cloned()
            .unwrap_or_else(|| default_prompt.to_string())
    }
}

#[async_trait]
impl AIProvider for OllamaProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn configured_model_ids(&self) -> Vec<String> {
        self.configured_models.keys().cloned().collect()
    }

    fn configured_model_details(&self) -> Vec<super::super::provider::ConfiguredModelInfo> {
        self.configured_models
            .iter()
            .map(|(id, config)| super::super::provider::ConfiguredModelInfo {
                id: id.clone(),
                backend_model_name: config.ollama_model.clone(),
                description: config.description.clone(),
                supported_languages: config.supported_languages.clone(),
            })
            .collect()
    }

    async fn check_health(&self) -> AIProviderResult<HealthStatus> {
        let url = self.api_url("/api/tags");

        match self.client.get(&url).send().await {
            Ok(response) => {
                if response.status().is_success() {
                    // Try to parse the response to count models
                    let model_count = response
                        .json::<OllamaTagsResponse>()
                        .await
                        .map(|r| r.models.len())
                        .ok();

                    Ok(HealthStatus {
                        healthy: true,
                        message: Some("Ollama is running".to_string()),
                        available_models: model_count,
                    })
                } else {
                    Ok(HealthStatus {
                        healthy: false,
                        message: Some(format!(
                            "Ollama returned status {}",
                            response.status().as_u16()
                        )),
                        available_models: None,
                    })
                }
            }
            Err(e) => {
                if e.is_timeout() {
                    Err(AIProviderError::Timeout {
                        provider: self.name.clone(),
                        timeout_secs: self.timeout.as_secs(),
                    })
                } else if e.is_connect() {
                    Err(AIProviderError::Unavailable {
                        provider: self.name.clone(),
                        message: format!("Cannot connect to Ollama at {}", self.base_url),
                    })
                } else {
                    Err(AIProviderError::RequestFailed {
                        provider: self.name.clone(),
                        message: e.to_string(),
                    })
                }
            }
        }
    }

    async fn list_models(&self, vision_only: bool) -> AIProviderResult<Vec<ModelInfo>> {
        let url = self.api_url("/api/tags");

        let response = self.client.get(&url).send().await.map_err(|e| {
            if e.is_timeout() {
                AIProviderError::Timeout {
                    provider: self.name.clone(),
                    timeout_secs: self.timeout.as_secs(),
                }
            } else if e.is_connect() {
                AIProviderError::Unavailable {
                    provider: self.name.clone(),
                    message: format!("Cannot connect to Ollama at {}", self.base_url),
                }
            } else {
                AIProviderError::RequestFailed {
                    provider: self.name.clone(),
                    message: e.to_string(),
                }
            }
        })?;

        if !response.status().is_success() {
            return Err(AIProviderError::RequestFailed {
                provider: self.name.clone(),
                message: format!("Ollama returned status {}", response.status().as_u16()),
            });
        }

        let tags_response: OllamaTagsResponse =
            response
                .json()
                .await
                .map_err(|e| AIProviderError::InvalidResponse {
                    provider: self.name.clone(),
                    message: format!("Failed to parse Ollama response: {}", e),
                })?;

        let models: Vec<ModelInfo> = tags_response
            .models
            .into_iter()
            .map(|m| {
                let supports_vision = m.likely_supports_vision();
                ModelInfo {
                    id: m.name.clone(),
                    name: m.name.clone(),
                    description: m.details.as_ref().and_then(|d| {
                        d.parameter_size
                            .as_ref()
                            .map(|ps| format!("{} parameters", ps))
                    }),
                    supports_vision,
                    provider: self.name.clone(),
                }
            })
            .filter(|m| !vision_only || m.supports_vision)
            .collect();

        Ok(models)
    }

    async fn analyze_image(
        &self,
        request: AnalyzeImageRequest,
    ) -> AIProviderResult<AnalyzeImageResponse> {
        // Resolve the model name - either from config or use directly
        let ollama_model = self
            .get_ollama_model_name(&request.model)
            .map(|s| s.to_string())
            .unwrap_or_else(|| request.model.clone());

        // Get the prompt (from config template or use provided), then resolve
        // the {language} placeholder so both config and default prompts get
        // the correct language substitution.
        let language = request.language.as_deref().unwrap_or("English");
        let prompt = self
            .get_prompt_for_model(&request.model, &request.prompt)
            .replace("{language}", language);

        debug!(prompt);

        let ollama_request = OllamaGenerateRequest {
            model: ollama_model.clone(),
            prompt,
            images: Some(vec![request.image_base64]),
            stream: false,
            options: None,
        };

        let url = self.api_url("/api/generate");

        let response = self
            .client
            .post(&url)
            .json(&ollama_request)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    AIProviderError::Timeout {
                        provider: self.name.clone(),
                        timeout_secs: self.timeout.as_secs(),
                    }
                } else if e.is_connect() {
                    AIProviderError::Unavailable {
                        provider: self.name.clone(),
                        message: format!("Cannot connect to Ollama at {}", self.base_url),
                    }
                } else {
                    AIProviderError::RequestFailed {
                        provider: self.name.clone(),
                        message: e.to_string(),
                    }
                }
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();

            // Check for model not found error
            if status.as_u16() == 404 || body.contains("model") && body.contains("not found") {
                return Err(AIProviderError::ModelNotFound {
                    provider: self.name.clone(),
                    model: ollama_model,
                });
            }

            return Err(AIProviderError::RequestFailed {
                provider: self.name.clone(),
                message: format!("Ollama returned status {}: {}", status.as_u16(), body),
            });
        }

        let ollama_response: OllamaGenerateResponse =
            response
                .json()
                .await
                .map_err(|e| AIProviderError::InvalidResponse {
                    provider: self.name.clone(),
                    message: format!("Failed to parse Ollama response: {}", e),
                })?;

        // Build token usage if available
        let tokens_used = match (
            ollama_response.prompt_eval_count,
            ollama_response.eval_count,
        ) {
            (Some(prompt_tokens), Some(completion_tokens)) => Some(TokenUsage {
                prompt_tokens,
                completion_tokens,
                total_tokens: prompt_tokens + completion_tokens,
            }),
            _ => None,
        };

        Ok(AnalyzeImageResponse {
            text: ollama_response.response,
            model: ollama_response.model,
            tokens_used,
        })
    }
}

/// Creates a new Ollama provider from configuration.
pub fn create_ollama_provider(
    name: impl Into<String>,
    config: &crate::config::OllamaProviderConfig,
) -> Arc<dyn AIProvider> {
    Arc::new(OllamaProvider::new(
        name,
        &config.base_url,
        config.timeout_seconds,
        config.models.clone(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_url() {
        let provider = OllamaProvider::new("test", "http://localhost:11434", 30, HashMap::new());

        assert_eq!(
            provider.api_url("/api/tags"),
            "http://localhost:11434/api/tags"
        );
        assert_eq!(
            provider.api_url("/api/generate"),
            "http://localhost:11434/api/generate"
        );
    }

    #[test]
    fn test_get_ollama_model_name() {
        let mut models = HashMap::new();
        models.insert(
            "qwen3-vl".to_string(),
            OllamaModelConfig {
                ollama_model: "qwen3-vl:8b".to_string(),
                prompt_template: None,
                description: None,
                supports_vision: true,
                supported_languages: vec![],
            },
        );

        let provider = OllamaProvider::new("test", "http://localhost:11434", 30, models);

        assert_eq!(
            provider.get_ollama_model_name("qwen3-vl"),
            Some("qwen3-vl:8b")
        );
        assert_eq!(provider.get_ollama_model_name("unknown"), None);
    }

    #[test]
    fn test_get_prompt_for_model() {
        let mut models = HashMap::new();
        models.insert(
            "qwen3-vl".to_string(),
            OllamaModelConfig {
                ollama_model: "qwen3-vl:8b".to_string(),
                prompt_template: Some("Custom prompt in {language}".to_string()),
                description: None,
                supports_vision: true,
                supported_languages: vec![],
            },
        );

        let provider = OllamaProvider::new("test", "http://localhost:11434", 30, models);

        assert_eq!(
            provider.get_prompt_for_model("qwen3-vl", "default"),
            "Custom prompt in {language}"
        );
        assert_eq!(
            provider.get_prompt_for_model("unknown", "default"),
            "default"
        );
    }
}
