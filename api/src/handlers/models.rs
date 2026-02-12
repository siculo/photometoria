use axum::Json;
use axum::extract::State;
use serde::Serialize;

use crate::app_state::AppState;
use crate::handlers::app_error::AppError;
use crate::services::ai::ModelInfo;

#[derive(Serialize)]
pub struct ModelsResponse {
    pub models: Vec<ModelInfo>,
}

/// Returns the list of AI models currently available in Ollama.
///
/// Only vision-capable models are returned, since the API is used for image analysis.
/// If Ollama is unreachable, returns a service unavailable error.
pub async fn list_models(
    State(state): State<AppState>,
) -> Result<Json<ModelsResponse>, AppError> {
    let provider = state
        .ai_providers
        .default_provider()
        .map_err(|e| AppError::internal_error(e.to_string()))?;

    let models = provider
        .list_models(true)
        .await
        .map_err(|e| AppError::service_unavailable(e.to_string()))?;

    Ok(Json(ModelsResponse { models }))
}
