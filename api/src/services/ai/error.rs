// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 The Photometoria contributors

//! AI provider error types.

use std::fmt;

/// Result type alias for AI provider operations.
pub type AIProviderResult<T> = Result<T, AIProviderError>;

/// Errors that can occur when interacting with AI providers.
#[derive(Debug)]
pub enum AIProviderError {
    /// Provider is not available or unreachable.
    Unavailable { provider: String, message: String },

    /// Request to the provider failed.
    RequestFailed { provider: String, message: String },

    /// Provider returned an invalid response.
    InvalidResponse { provider: String, message: String },

    /// The requested model was not found in the provider configuration.
    ModelNotFound { provider: String, model: String },

    /// The model is configured but not currently available in the provider backend.
    ModelNotAvailable { provider: String, model: String },

    /// The model does not support vision/image analysis.
    VisionNotSupported { provider: String, model: String },

    /// Configuration error for the provider.
    ConfigurationError { provider: String, message: String },

    /// Request timed out.
    Timeout { provider: String, timeout_secs: u64 },

    /// Image encoding/decoding error.
    ImageError { message: String },
}

impl fmt::Display for AIProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable { provider, message } => {
                write!(f, "AI provider '{}' is unavailable: {}", provider, message)
            }
            Self::RequestFailed { provider, message } => {
                write!(
                    f,
                    "Request to AI provider '{}' failed: {}",
                    provider, message
                )
            }
            Self::InvalidResponse { provider, message } => {
                write!(
                    f,
                    "Invalid response from AI provider '{}': {}",
                    provider, message
                )
            }
            Self::ModelNotFound { provider, model } => {
                write!(
                    f,
                    "Model '{}' not found in AI provider '{}'",
                    model, provider
                )
            }
            Self::ModelNotAvailable { provider, model } => {
                write!(
                    f,
                    "Model '{}' is not available in AI provider '{}'",
                    model, provider
                )
            }
            Self::VisionNotSupported { provider, model } => {
                write!(
                    f,
                    "Model '{}' in provider '{}' does not support vision",
                    model, provider
                )
            }
            Self::ConfigurationError { provider, message } => {
                write!(
                    f,
                    "Configuration error for AI provider '{}': {}",
                    provider, message
                )
            }
            Self::Timeout {
                provider,
                timeout_secs,
            } => {
                write!(
                    f,
                    "Request to AI provider '{}' timed out after {} seconds",
                    provider, timeout_secs
                )
            }
            Self::ImageError { message } => {
                write!(f, "Image processing error: {}", message)
            }
        }
    }
}

impl std::error::Error for AIProviderError {}
