// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 The Photometoria contributors

//! AI provider trait and common types.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use super::error::AIProviderResult;

/// Information about an AI model available from a provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    /// Unique identifier for the model within the provider.
    pub id: String,

    /// Human-readable name for the model.
    pub name: String,

    /// Optional description of the model's capabilities.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Whether this model supports vision/image analysis.
    pub supports_vision: bool,

    /// The provider that offers this model.
    pub provider: String,
}

/// Request to analyze an image using an AI model.
#[derive(Debug, Clone)]
pub struct AnalyzeImageRequest {
    /// The model ID to use for analysis.
    pub model: String,

    /// Base64-encoded image data.
    pub image_base64: String,

    /// The prompt to send with the image.
    pub prompt: String,

    /// The language for tag generation (e.g., "English", "Italian").
    /// Used to resolve `{language}` placeholders in the prompt template.
    pub language: Option<String>,
}

/// Response from an image analysis request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyzeImageResponse {
    /// The generated text response from the model.
    pub text: String,

    /// The model that was used for analysis.
    pub model: String,

    /// Optional token usage statistics.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens_used: Option<TokenUsage>,
}

/// Token usage statistics from an AI request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    /// Number of tokens in the prompt.
    pub prompt_tokens: u32,

    /// Number of tokens in the completion.
    pub completion_tokens: u32,

    /// Total tokens used.
    pub total_tokens: u32,
}

/// Health status of an AI provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    /// Whether the provider is healthy and available.
    pub healthy: bool,

    /// Optional message with additional details.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,

    /// Number of available models (if known).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub available_models: Option<usize>,
}

/// Information about a model as configured in the provider settings.
///
/// Unlike `ModelInfo` (which comes from a live Ollama query), this is derived
/// purely from the static configuration and requires no network call.
#[derive(Debug, Clone)]
pub struct ConfiguredModelInfo {
    /// The model ID as used in API requests (config key, e.g., "qwen3-vl").
    pub id: String,

    /// The actual model name as known to the backend (e.g., "qwen3-vl:8b").
    ///
    /// Used to cross-reference against the list of installed models.
    pub backend_model_name: String,

    /// Optional human-readable description from the configuration.
    pub description: Option<String>,

    /// Languages this model has been tested to support.
    pub supported_languages: Vec<String>,
}

/// Trait for AI providers that can analyze images and generate metadata.
///
/// This trait defines the interface that all AI providers must implement.
/// It enables the system to work with different AI backends (Ollama, OpenAI, etc.)
/// through a unified interface.
#[async_trait]
pub trait AIProvider: Send + Sync {
    /// Returns the unique name of this provider.
    fn name(&self) -> &str;

    /// Returns the model IDs configured for this provider.
    ///
    /// These are the keys from the provider's model configuration — the IDs
    /// that callers pass to `analyze_image`. No network call is made.
    fn configured_model_ids(&self) -> Vec<String>;

    /// Returns details (id + description) for all configured models.
    ///
    /// No network call is made; data comes from static configuration.
    fn configured_model_details(&self) -> Vec<ConfiguredModelInfo>;

    /// Checks the health/availability of the provider.
    ///
    /// This should verify that the provider is reachable and operational.
    async fn check_health(&self) -> AIProviderResult<HealthStatus>;

    /// Lists available models from this provider.
    ///
    /// # Arguments
    ///
    /// * `vision_only` - If true, only return models that support vision/image analysis.
    async fn list_models(&self, vision_only: bool) -> AIProviderResult<Vec<ModelInfo>>;

    /// Analyzes an image using the specified model.
    ///
    /// # Arguments
    ///
    /// * `request` - The analysis request containing the model, image, and prompt.
    async fn analyze_image(
        &self,
        request: AnalyzeImageRequest,
    ) -> AIProviderResult<AnalyzeImageResponse>;
}
