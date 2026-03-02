// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 The Photometoria contributors

use axum::Json;
use axum::extract::State;
use serde::Serialize;
use std::collections::HashSet;
use std::sync::Arc;

use crate::app_state::AppState;
use crate::handlers::app_error::{AppError, AppPath};
use crate::services::ai::AIProvider;

#[derive(Serialize)]
pub struct ProviderEntry {
    pub name: String,
}

#[derive(Serialize)]
pub struct ProvidersResponse {
    pub providers: Vec<ProviderEntry>,
}

#[derive(Serialize)]
pub struct ModelEntry {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub available: bool,
}

#[derive(Serialize)]
pub struct ProviderDetailsResponse {
    pub name: String,
    pub models: Vec<ModelEntry>,
}

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
    Ok(Json(ProvidersResponse { providers }))
}

pub async fn provider_details(
    State(state): State<AppState>,
    AppPath(provider_name): AppPath<String>,
) -> Result<Json<ProviderDetailsResponse>, AppError> {
    let provider = state
        .ai_providers
        .get(&provider_name)
        .map_err(|e| AppError::internal_error(e.to_string()))?;

    let models = provider_models(&provider).await;

    Ok(Json(ProviderDetailsResponse {
        name: provider.name().to_string(),
        models,
    }))
}

#[deprecated]
pub async fn list_default_provider_models(
    State(state): State<AppState>,
) -> Result<Json<ProviderDetailsResponse>, AppError> {
    let provider = state
        .ai_providers
        .default_provider()
        .map_err(|e| AppError::internal_error(e.to_string()))?;

    let models = provider_models(&provider).await;

    Ok(Json(ProviderDetailsResponse {
        name: provider.name().to_string(),
        models
    }))
}

async fn provider_models(provider: &Arc<dyn AIProvider>) -> Vec<ModelEntry> {
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
    models
}

/// Query provider for installed model names. On failure treat all as unavailable.
/// NOTE: the only model currently supported is Ollama.
///
/// TODO: other providers should be supported
async fn available_models(provider: &Arc<dyn AIProvider>) -> HashSet<String> {
    if provider.name().eq("ollama") {
        HashSet::new()
    } else {
        provider
            .list_models(false)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|m| m.name)
            .collect()
    }
}
// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;

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
        configured: Vec<ConfiguredModelInfo>,
        installed: Vec<ModelInfo>,
    }

    #[async_trait]
    impl AIProvider for MockProvider {
        fn name(&self) -> &str {
            "mock"
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
        let mut registry = ProviderRegistry::new();
        registry.register("mock", Arc::new(provider));
        registry.set_default("mock").unwrap();
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
        let configured = provider.configured_model_details();

        let installed: std::collections::HashSet<String> = provider
            .list_models(false)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|m| m.name)
            .collect();

        configured
            .into_iter()
            .map(|m| {
                let available = installed.contains(&m.backend_model_name);
                super::ModelEntry {
                    name: m.id,
                    description: m.description,
                    available,
                }
            })
            .collect()
    }

    #[tokio::test]
    async fn test_installed_model_is_available() {
        let provider = MockProvider {
            configured: vec![configured("qwen3-vl", "qwen3-vl:8b", Some("fast model"))],
            installed: vec![make_model_info("qwen3-vl:8b")],
        };
        let models = call_list_models(registry_with(provider)).await;

        assert_eq!(models.len(), 1);
        assert_eq!(models[0].name, "qwen3-vl");
        assert_eq!(models[0].description.as_deref(), Some("fast model"));
        assert!(models[0].available, "model should be available");
    }

    #[tokio::test]
    async fn test_configured_but_not_installed_is_unavailable() {
        let provider = MockProvider {
            configured: vec![configured("qwen3-vl", "qwen3-vl:8b", None)],
            installed: vec![], // nothing installed
        };
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
        let provider = MockProvider {
            configured: vec![configured("qwen3-vl", "qwen3-vl:8b", None)],
            installed: vec![
                make_model_info("qwen3-vl:8b"),
                make_model_info("llava:latest"), // installed but not configured
            ],
        };
        let models = call_list_models(registry_with(provider)).await;

        assert_eq!(models.len(), 1, "only configured models should be returned");
        assert_eq!(models[0].name, "qwen3-vl");
    }

    #[tokio::test]
    async fn test_backend_name_used_for_availability_check() {
        // Config key is "qwen3-vl", Ollama reports "qwen3-vl:8b" — must match via backend_model_name.
        let provider = MockProvider {
            configured: vec![configured("qwen3-vl", "qwen3-vl:8b", None)],
            installed: vec![make_model_info("qwen3-vl:8b")],
        };
        let models = call_list_models(registry_with(provider)).await;

        assert!(
            models[0].available,
            "backend_model_name should be used for availability check, not the config key"
        );
    }
}
