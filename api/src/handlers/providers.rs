// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 The Photometoria contributors

//! Handlers for AI provider discovery and model listing.
//!
//! Provides endpoints to enumerate registered AI providers and inspect
//! the models each provider exposes, including real-time availability
//! checks against the provider backend.

use axum::Json;
use axum::extract::State;
use serde::Serialize;
use std::collections::HashSet;
use std::sync::Arc;

use crate::app_state::AppState;
use crate::handlers::app_error::{AppError, AppPath};
use crate::services::ai::AIProvider;

/// Single entry in the provider list returned by [`list_providers`].
#[derive(Debug, Serialize)]
pub struct ProviderEntry {
    pub name: String,
}

/// Response body for `GET /api/providers`.
#[derive(Debug, Serialize)]
pub struct ProvidersResponse {
    pub providers: Vec<ProviderEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
}

/// A model exposed by a provider, with its current availability status.
#[derive(Debug, Serialize)]
pub struct ModelEntry {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub available: bool,
}

/// Response body for `GET /api/providers/{provider_name}`.
#[derive(Debug, Serialize)]
pub struct ProviderDetailsResponse {
    pub name: String,
    pub models: Vec<ModelEntry>,
}

/// Lists all registered AI providers.
///
/// Returns the name of each provider currently registered in the
/// [`ProviderRegistry`](crate::services::ai::ProviderRegistry).
pub async fn list_providers(
    State(state): State<AppState>,
) -> Result<Json<ProvidersResponse>, AppError> {
    let providers: Vec<ProviderEntry> = state
        .ai_providers
        .providers()
        .iter()
        .map(|provider| ProviderEntry {
            name: provider.name().to_string(),
        })
        .collect();
    let default = state
        .ai_providers
        .default_provider_name()
        .map(str::to_string);
    Ok(Json(ProvidersResponse { providers, default }))
}

/// Returns details for a single provider, including its configured models
/// and their current availability.
///
/// Responds with `404 Not Found` if the provider name is not registered.
pub async fn provider_details(
    State(state): State<AppState>,
    AppPath(provider_name): AppPath<String>,
) -> Result<Json<ProviderDetailsResponse>, AppError> {
    let provider = state
        .ai_providers
        .get(&provider_name)
        .map_err(|e| AppError::not_found(e.to_string()))?;

    Ok(Json(provider_response(&provider).await))
}

/// Returns models for the default provider.
///
/// Deprecated in favour of [`provider_details`], which accepts an explicit
/// provider name. Scheduled for removal in #32.
#[deprecated]
pub async fn list_default_provider_models(
    State(state): State<AppState>,
) -> Result<Json<ProviderDetailsResponse>, AppError> {
    let provider = state
        .ai_providers
        .default_provider()
        .map_err(|e| AppError::internal_error(e.to_string()))?;

    Ok(Json(provider_response(&provider).await))
}

/// Builds a [`ProviderDetailsResponse`] for the given provider by cross-referencing
/// configured models against those actually installed on the backend.
async fn provider_response(provider: &Arc<dyn AIProvider>) -> ProviderDetailsResponse {
    let configured = provider.configured_model_details();
    let installed: HashSet<String> = available_models(provider).await;

    let models = configured
        .into_iter()
        .map(|m| {
            let available = installed.contains(&m.backend_model_name);
            ModelEntry {
                name: m.id,
                description: m.description,
                available,
            }
        })
        .collect();

    ProviderDetailsResponse {
        name: provider.name().to_string(),
        models,
    }
}

/// Query provider for installed model names. On failure treat all as unavailable.
async fn available_models(provider: &Arc<dyn AIProvider>) -> HashSet<String> {
    provider
        .list_models(false)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|m| m.name)
        .collect()
}
// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;

    use axum::extract::State;

    use crate::handlers::app_error::AppPath;
    use crate::handlers::test_utils::fixtures::create_test_state;
    use crate::services::ai::{
        AIProvider, AIProviderError, AIProviderResult, AnalyzeImageRequest, AnalyzeImageResponse,
        ConfiguredModelInfo, HealthStatus, ModelInfo, ProviderRegistry,
    };

    fn make_model_info(name: &str) -> ModelInfo {
        ModelInfo {
            id: name.to_string(),
            name: name.to_string(),
            description: None,
            supports_vision: true,
            provider: "test".to_string(),
        }
    }

    struct MockProvider {
        provider_name: String,
        configured: Vec<ConfiguredModelInfo>,
        installed: Vec<ModelInfo>,
    }

    impl MockProvider {
        fn new(configured: Vec<ConfiguredModelInfo>, installed: Vec<ModelInfo>) -> Self {
            Self {
                provider_name: "mock".to_string(),
                configured,
                installed,
            }
        }

        fn named(name: &str) -> Self {
            Self {
                provider_name: name.to_string(),
                configured: vec![],
                installed: vec![],
            }
        }
    }

    #[async_trait]
    impl AIProvider for MockProvider {
        fn name(&self) -> &str {
            &self.provider_name
        }

        fn configured_model_ids(&self) -> Vec<String> {
            self.configured.iter().map(|m| m.id.clone()).collect()
        }

        fn configured_model_details(&self) -> Vec<ConfiguredModelInfo> {
            self.configured.clone()
        }

        async fn check_health(&self) -> AIProviderResult<HealthStatus> {
            unimplemented!()
        }

        async fn list_models(&self, _vision_only: bool) -> AIProviderResult<Vec<ModelInfo>> {
            Ok(self.installed.clone())
        }

        async fn analyze_image(
            &self,
            _request: AnalyzeImageRequest,
        ) -> AIProviderResult<AnalyzeImageResponse> {
            unimplemented!()
        }
    }

    struct UnavailableProvider {
        configured: Vec<ConfiguredModelInfo>,
    }

    #[async_trait]
    impl AIProvider for UnavailableProvider {
        fn name(&self) -> &str {
            "unavailable"
        }

        fn configured_model_ids(&self) -> Vec<String> {
            self.configured.iter().map(|m| m.id.clone()).collect()
        }

        fn configured_model_details(&self) -> Vec<ConfiguredModelInfo> {
            self.configured.clone()
        }

        async fn check_health(&self) -> AIProviderResult<HealthStatus> {
            unimplemented!()
        }

        async fn list_models(&self, _vision_only: bool) -> AIProviderResult<Vec<ModelInfo>> {
            Err(AIProviderError::Unavailable {
                provider: "unavailable".into(),
                message: "Ollama not reachable".into(),
            })
        }

        async fn analyze_image(
            &self,
            _request: AnalyzeImageRequest,
        ) -> AIProviderResult<AnalyzeImageResponse> {
            unimplemented!()
        }
    }

    fn registry_with(provider: impl AIProvider + 'static) -> Arc<ProviderRegistry> {
        registry_with_name("mock", provider)
    }

    fn registry_with_name(
        name: &str,
        provider: impl AIProvider + 'static,
    ) -> Arc<ProviderRegistry> {
        let mut registry = ProviderRegistry::new();
        registry.register(name, Arc::new(provider));
        registry.set_default(name).unwrap();
        Arc::new(registry)
    }

    fn configured(id: &str, backend: &str, description: Option<&str>) -> ConfiguredModelInfo {
        ConfiguredModelInfo {
            id: id.to_string(),
            backend_model_name: backend.to_string(),
            description: description.map(str::to_string),
        }
    }

    /// Calls the handler logic directly without HTTP, reusing the same
    /// registry + provider wiring used in production.
    async fn call_list_models(registry: Arc<ProviderRegistry>) -> Vec<super::ModelEntry> {
        let provider = registry.default_provider().unwrap();
        super::provider_response(&provider).await.models
    }

    #[tokio::test]
    async fn test_installed_model_is_available() {
        let provider = MockProvider::new(
            vec![configured("qwen3-vl", "qwen3-vl:8b", Some("fast model"))],
            vec![make_model_info("qwen3-vl:8b")],
        );
        let models = call_list_models(registry_with(provider)).await;

        assert_eq!(models.len(), 1);
        assert_eq!(models[0].name, "qwen3-vl");
        assert_eq!(models[0].description.as_deref(), Some("fast model"));
        assert!(models[0].available, "model should be available");
    }

    #[tokio::test]
    async fn test_ollama_provider_installed_model_is_available() {
        let provider = MockProvider {
            provider_name: "ollama".to_string(),
            configured: vec![configured("qwen3-vl", "qwen3-vl:8b", None)],
            installed: vec![make_model_info("qwen3-vl:8b")],
        };
        let models = call_list_models(registry_with_name("ollama", provider)).await;

        assert!(
            models[0].available,
            "ollama provider must report installed models as available"
        );
    }

    #[tokio::test]
    async fn test_configured_but_not_installed_is_unavailable() {
        let provider = MockProvider::new(vec![configured("qwen3-vl", "qwen3-vl:8b", None)], vec![]);
        let models = call_list_models(registry_with(provider)).await;

        assert_eq!(models.len(), 1);
        assert!(!models[0].available, "model should be unavailable");
    }

    #[tokio::test]
    async fn test_ollama_unreachable_all_unavailable() {
        let provider = UnavailableProvider {
            configured: vec![
                configured("qwen3-vl", "qwen3-vl:8b", None),
                configured("llava", "llava:latest", None),
            ],
        };
        let models = call_list_models(registry_with(provider)).await;

        assert_eq!(models.len(), 2);
        assert!(!models[0].available);
        assert!(!models[1].available);
    }

    #[tokio::test]
    async fn test_only_configured_models_are_returned() {
        // Ollama has extra models not in config — they must not appear in response.
        let provider = MockProvider::new(
            vec![configured("qwen3-vl", "qwen3-vl:8b", None)],
            vec![
                make_model_info("qwen3-vl:8b"),
                make_model_info("llava:latest"), // installed but not configured
            ],
        );
        let models = call_list_models(registry_with(provider)).await;

        assert_eq!(models.len(), 1, "only configured models should be returned");
        assert_eq!(models[0].name, "qwen3-vl");
    }

    #[tokio::test]
    async fn test_backend_name_used_for_availability_check() {
        // Config key is "qwen3-vl", Ollama reports "qwen3-vl:8b" — must match via backend_model_name.
        let provider = MockProvider::new(
            vec![configured("qwen3-vl", "qwen3-vl:8b", None)],
            vec![make_model_info("qwen3-vl:8b")],
        );
        let models = call_list_models(registry_with(provider)).await;

        assert!(
            models[0].available,
            "backend_model_name should be used for availability check, not the config key"
        );
    }

    // ---- list_providers handler tests ----

    #[tokio::test]
    async fn test_list_providers_returns_registered_provider() {
        let ts = create_test_state().await;

        let result = super::list_providers(State(ts.state)).await;

        let response = result.unwrap().0;
        assert_eq!(response.providers.len(), 1);
        assert_eq!(response.providers[0].name, "test");
        assert_eq!(response.default.as_deref(), Some("test"));
    }

    #[tokio::test]
    async fn test_list_providers_empty_registry() {
        let mut ts = create_test_state().await;
        ts.state.ai_providers = Arc::new(ProviderRegistry::new());

        let result = super::list_providers(State(ts.state)).await;

        let response = result.unwrap().0;
        assert!(response.providers.is_empty());
        assert!(response.default.is_none());
    }

    #[tokio::test]
    async fn test_list_providers_multiple_providers() {
        let mut ts = create_test_state().await;
        let mut registry = ProviderRegistry::new();
        registry.register("alpha", Arc::new(MockProvider::named("alpha")));
        registry.register("beta", Arc::new(MockProvider::named("beta")));
        registry.set_default("beta").unwrap();
        ts.state.ai_providers = Arc::new(registry);

        let result = super::list_providers(State(ts.state)).await;

        let response = result.unwrap().0;
        let mut names: Vec<String> = response.providers.into_iter().map(|p| p.name).collect();
        names.sort();
        assert_eq!(names, vec!["alpha", "beta"]);
        assert_eq!(response.default.as_deref(), Some("beta"));
    }

    // ---- provider_details handler tests ----

    #[tokio::test]
    async fn test_provider_details_known_provider() {
        let mut ts = create_test_state().await;
        let provider = MockProvider::new(
            vec![configured("qwen3-vl", "qwen3-vl:8b", Some("fast model"))],
            vec![make_model_info("qwen3-vl:8b")],
        );
        let mut registry = ProviderRegistry::new();
        registry.register("mock", Arc::new(provider));
        ts.state.ai_providers = Arc::new(registry);

        let result = super::provider_details(State(ts.state), AppPath("mock".to_string())).await;

        let response = result.unwrap().0;
        assert_eq!(response.name, "mock");
        assert_eq!(response.models.len(), 1);
        assert_eq!(response.models[0].name, "qwen3-vl");
        assert!(response.models[0].available);
    }

    #[tokio::test]
    async fn test_provider_details_unknown_provider_returns_not_found() {
        let ts = create_test_state().await;

        let result =
            super::provider_details(State(ts.state), AppPath("nonexistent".to_string())).await;

        let error = result.unwrap_err();
        assert_eq!(error.status, axum::http::StatusCode::NOT_FOUND);
    }
}
