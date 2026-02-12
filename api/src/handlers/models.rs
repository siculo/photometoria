use axum::Json;
use axum::extract::State;
use serde::Serialize;

use crate::app_state::AppState;
use crate::handlers::app_error::AppError;

#[derive(Serialize)]
pub struct ModelEntry {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub available: bool,
}

#[derive(Serialize)]
pub struct ModelsResponse {
    pub models: Vec<ModelEntry>,
}

/// Returns the list of configured AI models with their availability in Ollama.
///
/// Each model comes from the static provider configuration. The `available` field
/// indicates whether the model is currently installed and reachable in Ollama.
/// If Ollama is unreachable, all models are returned with `available: false`.
pub async fn list_models(
    State(state): State<AppState>,
) -> Result<Json<ModelsResponse>, AppError> {
    let provider = state
        .ai_providers
        .default_provider()
        .map_err(|e| AppError::internal_error(e.to_string()))?;

    let configured = provider.configured_model_details();

    // Query Ollama for installed models; on failure treat all as unavailable.
    let installed_names: std::collections::HashSet<String> = provider
        .list_models(false)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|m| m.name)
        .collect();

    let models = configured
        .into_iter()
        .map(|m| {
            let available = installed_names.contains(&m.id);
            ModelEntry {
                name: m.id,
                description: m.description,
                available,
            }
        })
        .collect();

    Ok(Json(ModelsResponse { models }))
}
