//! AI provider abstraction layer.
//!
//! This module provides a unified interface for interacting with different AI providers
//! (Ollama, OpenAI, etc.) through a common trait and registry system.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────┐
//! │            Configurazione               │
//! │  [ai.providers.ollama] → OllamaConfig   │
//! ├─────────────────────────────────────────┤
//! │               Trait                     │
//! │  AIProvider (list_models, analyze_image,│
//! │              check_health)              │
//! ├─────────────────────────────────────────┤
//! │           Implementazione               │
//! │  OllamaProvider → chiama Ollama API     │
//! ├─────────────────────────────────────────┤
//! │              Registry                   │
//! │  HashMap<String, Arc<dyn AIProvider>>   │
//! │  + default_provider                     │
//! └─────────────────────────────────────────┘
//! ```
//!
//! # Usage
//!
//! ```ignore
//! use photometoria::services::ai::{ProviderRegistry, AIProvider, AnalyzeImageRequest};
//!
//! // Get the default provider
//! let provider = registry.default_provider()?;
//!
//! // Or get a specific provider by name
//! let ollama = registry.get("ollama")?;
//!
//! // List available models
//! let models = provider.list_models(true).await?; // vision_only = true
//!
//! // Analyze an image
//! let request = AnalyzeImageRequest {
//!     model: "qwen3-vl:8b".to_string(),
//!     image_base64: base64_data,
//!     prompt: "Describe this image".to_string(),
//! };
//! let response = provider.analyze_image(request).await?;
//! ```

pub mod error;
pub mod ollama;
pub mod provider;
pub mod registry;

// Re-export main types for convenience
pub use error::{AIProviderError, AIProviderResult};
pub use provider::{
    AIProvider, AnalyzeImageRequest, AnalyzeImageResponse, HealthStatus, ModelInfo, TokenUsage,
};
pub use registry::ProviderRegistry;
