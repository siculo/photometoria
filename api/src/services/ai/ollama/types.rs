// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 The Photometoria contributors

//! Ollama API types.
//!
//! These types map to the Ollama REST API responses and requests.
//! See: https://github.com/ollama/ollama/blob/main/docs/api.md

use serde::{Deserialize, Serialize};

/// Request body for POST /api/generate.
#[derive(Debug, Clone, Serialize)]
pub struct OllamaGenerateRequest {
    /// The model name to use.
    pub model: String,

    /// The prompt to send to the model.
    pub prompt: String,

    /// Base64-encoded images for vision models.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub images: Option<Vec<String>>,

    /// Whether to stream the response (we always set this to false).
    pub stream: bool,

    /// Optional generation options.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<OllamaGenerateOptions>,
}

/// Generation options for Ollama.
#[derive(Debug, Clone, Default, Serialize)]
pub struct OllamaGenerateOptions {
    /// Temperature for sampling (0.0 - 1.0).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,

    /// Maximum number of tokens to generate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub num_predict: Option<i32>,
}

/// Response from POST /api/generate (non-streaming).
#[derive(Debug, Clone, Deserialize)]
pub struct OllamaGenerateResponse {
    /// The generated text response.
    pub response: String,

    /// The model used for generation.
    pub model: String,

    /// Whether the generation is complete.
    pub done: bool,

    /// Total duration in nanoseconds.
    #[serde(default)]
    pub total_duration: Option<u64>,

    /// Number of tokens in the prompt.
    #[serde(default)]
    pub prompt_eval_count: Option<u32>,

    /// Number of tokens generated.
    #[serde(default)]
    pub eval_count: Option<u32>,
}

/// Response from GET /api/tags (list models).
#[derive(Debug, Clone, Deserialize)]
pub struct OllamaTagsResponse {
    /// List of available models.
    pub models: Vec<OllamaModelInfo>,
}

/// Information about a single Ollama model.
#[derive(Debug, Clone, Deserialize)]
pub struct OllamaModelInfo {
    /// The model name (e.g., "llava:latest").
    pub name: String,

    /// When the model was last modified.
    #[serde(default)]
    pub modified_at: Option<String>,

    /// Size of the model in bytes.
    #[serde(default)]
    pub size: Option<u64>,

    /// Model digest (hash).
    #[serde(default)]
    pub digest: Option<String>,

    /// Model details.
    #[serde(default)]
    pub details: Option<OllamaModelDetails>,
}

/// Detailed information about an Ollama model.
#[derive(Debug, Clone, Deserialize)]
pub struct OllamaModelDetails {
    /// Parent model name.
    #[serde(default)]
    pub parent_model: Option<String>,

    /// Model format (e.g., "gguf").
    #[serde(default)]
    pub format: Option<String>,

    /// Model family (e.g., "llama").
    #[serde(default)]
    pub family: Option<String>,

    /// Model families this model belongs to.
    #[serde(default)]
    pub families: Option<Vec<String>>,

    /// Parameter size (e.g., "7B").
    #[serde(default)]
    pub parameter_size: Option<String>,

    /// Quantization level (e.g., "Q4_0").
    #[serde(default)]
    pub quantization_level: Option<String>,
}

impl OllamaModelInfo {
    /// Checks if this model likely supports vision based on its name/family.
    ///
    /// This is a heuristic since Ollama doesn't expose vision capability directly.
    pub fn likely_supports_vision(&self) -> bool {
        let name_lower = self.name.to_lowercase();

        // Known vision model families
        let vision_keywords = [
            "llava",
            "bakllava",
            "moondream",
            "qwen3-vl",
            "qwen-vl",
            "cogvlm",
            "fuyu",
            "obsidian",
            "vision",
        ];

        vision_keywords.iter().any(|kw| name_lower.contains(kw))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_likely_supports_vision() {
        let model = OllamaModelInfo {
            name: "llava:latest".to_string(),
            modified_at: None,
            size: None,
            digest: None,
            details: None,
        };
        assert!(model.likely_supports_vision());

        let model = OllamaModelInfo {
            name: "qwen3-vl:8b".to_string(),
            modified_at: None,
            size: None,
            digest: None,
            details: None,
        };
        assert!(model.likely_supports_vision());

        let model = OllamaModelInfo {
            name: "llama3:8b".to_string(),
            modified_at: None,
            size: None,
            digest: None,
            details: None,
        };
        assert!(!model.likely_supports_vision());
    }

    #[test]
    fn test_generate_request_serialization() {
        let request = OllamaGenerateRequest {
            model: "llava:latest".to_string(),
            prompt: "Describe this image".to_string(),
            images: Some(vec!["base64data".to_string()]),
            stream: false,
            options: None,
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"model\":\"llava:latest\""));
        assert!(json.contains("\"stream\":false"));
        assert!(json.contains("\"images\":[\"base64data\"]"));
    }
}
