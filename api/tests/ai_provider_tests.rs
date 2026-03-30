// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 The Photometoria contributors

//! Integration tests for AI providers using WireMock.

use std::collections::HashMap;

use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use photometoria_rest_api::config::{OllamaModelConfig, OllamaProviderConfig};
use photometoria_rest_api::services::ai::ollama::OllamaProvider;
use photometoria_rest_api::services::ai::{AIProvider, AIProviderError, AnalyzeImageRequest};

/// Creates an OllamaProvider configured to use the mock server.
fn create_test_provider(mock_server_uri: &str) -> OllamaProvider {
    let mut models = HashMap::new();
    models.insert(
        "test-vision".to_string(),
        OllamaModelConfig {
            ollama_model: "llava:latest".to_string(),
            prompt_template: Some("Describe this image".to_string()),
            description: Some("Test vision model".to_string()),
            supports_vision: true,
            supported_languages: vec![],
        },
    );

    OllamaProvider::new("test-ollama", mock_server_uri, 30, models)
}

// ============================================================================
// Health Check Tests
// ============================================================================

#[tokio::test]
async fn test_check_health_success() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/tags"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "models": [
                {"name": "llava:latest", "size": 400000000},
                {"name": "llama3:8b", "size": 500000000}
            ]
        })))
        .mount(&mock_server)
        .await;

    let provider = create_test_provider(&mock_server.uri());
    let health = provider.check_health().await.unwrap();

    assert!(health.healthy);
    assert_eq!(health.available_models, Some(2));
    assert!(health.message.is_some());
}

#[tokio::test]
async fn test_check_health_server_error() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/tags"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&mock_server)
        .await;

    let provider = create_test_provider(&mock_server.uri());
    let health = provider.check_health().await.unwrap();

    assert!(!health.healthy);
    assert!(health.message.unwrap().contains("500"));
}

#[tokio::test]
async fn test_check_health_connection_refused() {
    // Use a port that's not listening
    let provider = create_test_provider("http://127.0.0.1:59999");
    let result = provider.check_health().await;

    assert!(result.is_err());
    match result.unwrap_err() {
        AIProviderError::Unavailable { provider, .. } => {
            assert_eq!(provider, "test-ollama");
        }
        e => panic!("Expected Unavailable error, got: {:?}", e),
    }
}

// ============================================================================
// List Models Tests
// ============================================================================

#[tokio::test]
async fn test_list_models_all() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/tags"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "models": [
                {"name": "llava:latest", "size": 400000000},
                {"name": "qwen3-vl:8b", "size": 500000000},
                {"name": "llama3:8b", "size": 500000000}
            ]
        })))
        .mount(&mock_server)
        .await;

    let provider = create_test_provider(&mock_server.uri());
    let models = provider.list_models(false).await.unwrap();

    assert_eq!(models.len(), 3);
    assert!(models.iter().any(|m| m.id == "llava:latest"));
    assert!(models.iter().any(|m| m.id == "llama3:8b"));
}

#[tokio::test]
async fn test_list_models_vision_only() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/tags"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "models": [
                {"name": "llava:latest", "size": 400000000},
                {"name": "qwen3-vl:8b", "size": 500000000},
                {"name": "llama3:8b", "size": 500000000}
            ]
        })))
        .mount(&mock_server)
        .await;

    let provider = create_test_provider(&mock_server.uri());
    let models = provider.list_models(true).await.unwrap();

    // Should only return vision models (llava, qwen3-vl)
    assert_eq!(models.len(), 2);
    assert!(models.iter().all(|m| m.supports_vision));
    assert!(models.iter().any(|m| m.id == "llava:latest"));
    assert!(models.iter().any(|m| m.id == "qwen3-vl:8b"));
}

#[tokio::test]
async fn test_list_models_empty() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/tags"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "models": []
        })))
        .mount(&mock_server)
        .await;

    let provider = create_test_provider(&mock_server.uri());
    let models = provider.list_models(false).await.unwrap();

    assert!(models.is_empty());
}

// ============================================================================
// Analyze Image Tests
// ============================================================================

#[tokio::test]
async fn test_analyze_image_success() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/generate"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "model": "llava:latest",
            "response": "A beautiful sunset over the ocean with vibrant orange and purple colors.",
            "done": true,
            "prompt_eval_count": 100,
            "eval_count": 50
        })))
        .mount(&mock_server)
        .await;

    let provider = create_test_provider(&mock_server.uri());

    let request = AnalyzeImageRequest {
        model: "test-vision".to_string(), // Uses configured model mapping
        image_base64: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==".to_string(),
        prompt: "Describe this image".to_string(),
    };

    let response = provider.analyze_image(request).await.unwrap();

    assert_eq!(response.model, "llava:latest");
    assert!(response.text.contains("sunset"));
    assert!(response.tokens_used.is_some());

    let tokens = response.tokens_used.unwrap();
    assert_eq!(tokens.prompt_tokens, 100);
    assert_eq!(tokens.completion_tokens, 50);
    assert_eq!(tokens.total_tokens, 150);
}

#[tokio::test]
async fn test_analyze_image_model_not_found() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/generate"))
        .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
            "error": "model 'nonexistent' not found"
        })))
        .mount(&mock_server)
        .await;

    let provider = create_test_provider(&mock_server.uri());

    let request = AnalyzeImageRequest {
        model: "nonexistent".to_string(),
        image_base64: "base64data".to_string(),
        prompt: "Describe".to_string(),
    };

    let result = provider.analyze_image(request).await;

    assert!(result.is_err());
    match result.unwrap_err() {
        AIProviderError::ModelNotFound { provider, model } => {
            assert_eq!(provider, "test-ollama");
            assert_eq!(model, "nonexistent");
        }
        e => panic!("Expected ModelNotFound error, got: {:?}", e),
    }
}

#[tokio::test]
async fn test_analyze_image_uses_configured_model() {
    let mock_server = MockServer::start().await;

    // The mock will verify the request body contains the mapped model name
    Mock::given(method("POST"))
        .and(path("/api/generate"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "model": "llava:latest",
            "response": "Test response",
            "done": true
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    let provider = create_test_provider(&mock_server.uri());

    // Use the configured model alias "test-vision" which maps to "llava:latest"
    let request = AnalyzeImageRequest {
        model: "test-vision".to_string(),
        image_base64: "base64data".to_string(),
        prompt: "Test prompt".to_string(),
    };

    let response = provider.analyze_image(request).await.unwrap();
    assert_eq!(response.model, "llava:latest");
}

#[tokio::test]
async fn test_analyze_image_without_token_stats() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/generate"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "model": "llava:latest",
            "response": "Image description",
            "done": true
            // No token counts
        })))
        .mount(&mock_server)
        .await;

    let provider = create_test_provider(&mock_server.uri());

    let request = AnalyzeImageRequest {
        model: "llava:latest".to_string(),
        image_base64: "base64data".to_string(),
        prompt: "Describe".to_string(),
    };

    let response = provider.analyze_image(request).await.unwrap();

    assert!(response.tokens_used.is_none());
}

// ============================================================================
// Provider Registry Tests
// ============================================================================

#[tokio::test]
async fn test_provider_registry_from_config() {
    use photometoria_rest_api::config::{AIConfig, ProviderConfig};
    use photometoria_rest_api::services::ai::ProviderRegistry;

    let mock_server = MockServer::start().await;

    let mut providers = HashMap::new();
    providers.insert(
        "ollama".to_string(),
        ProviderConfig::Ollama(OllamaProviderConfig {
            base_url: mock_server.uri(),
            timeout_seconds: 30,
            devices: vec![],
            models: HashMap::new(),
        }),
    );

    let config = AIConfig {
        default_provider: Some("ollama".to_string()),
        default_language: None,
        providers,
    };

    let registry = ProviderRegistry::from_config(&config).unwrap();

    assert_eq!(registry.len(), 1);
    assert_eq!(registry.default_provider_name(), Some("ollama"));

    // Test that we can get the provider and it works
    Mock::given(method("GET"))
        .and(path("/api/tags"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "models": []
        })))
        .mount(&mock_server)
        .await;

    let provider = registry.default_provider().unwrap();
    let health = provider.check_health().await.unwrap();
    assert!(health.healthy);
}

#[tokio::test]
async fn test_provider_registry_invalid_default() {
    use photometoria_rest_api::config::AIConfig;
    use photometoria_rest_api::services::ai::ProviderRegistry;

    let config = AIConfig {
        default_provider: Some("nonexistent".to_string()),
        default_language: None,
        providers: HashMap::new(),
    };

    let result = ProviderRegistry::from_config(&config);
    assert!(result.is_err());
}
