//! Ollama AI provider module.
//!
//! This module implements the `AIProvider` trait for Ollama,
//! a local AI model runner that supports vision models.
//!
//! # Supported Ollama Endpoints
//!
//! - `GET /api/tags` - List available models
//! - `POST /api/generate` - Generate text (with optional images)
//!
//! # Example
//!
//! ```ignore
//! use photometoria::services::ai::ollama::create_ollama_provider;
//! use photometoria::config::OllamaProviderConfig;
//!
//! let config = OllamaProviderConfig::default();
//! let provider = create_ollama_provider("ollama", &config);
//!
//! // Check health
//! let health = provider.check_health().await?;
//!
//! // List vision models
//! let models = provider.list_models(true).await?;
//! ```

pub mod provider;
pub mod types;

pub use provider::{create_ollama_provider, OllamaProvider};
