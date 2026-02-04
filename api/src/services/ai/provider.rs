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

/// Trait for AI providers that can analyze images and generate metadata.
///
/// This trait defines the interface that all AI providers must implement.
/// It enables the system to work with different AI backends (Ollama, OpenAI, etc.)
/// through a unified interface.
#[async_trait]
pub trait AIProvider: Send + Sync {
    /// Returns the unique name of this provider.
    fn name(&self) -> &str;

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
    async fn analyze_image(&self, request: AnalyzeImageRequest) -> AIProviderResult<AnalyzeImageResponse>;
}
