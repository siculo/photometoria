// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 The Photometoria contributors

use crate::app_state::AppState;
use crate::handlers::app_error::AppError;
use crate::handlers::upload_photos::ALLOWED_MIME_TYPES;
use crate::models::info::{GeneralInfo, InfoResult, LimitsInfo, ServerInfo};
use crate::storage::JobStore;
use axum::Json;
use axum::extract::State;
use std::sync::Arc;

pub async fn info(State(state): State<AppState>) -> Result<Json<InfoResult>, AppError> {
    let version = env!("CARGO_PKG_VERSION");

    let allocated_space_bytes = state.config.storage_max_size();
    let used_space_bytes = state
        .photo_store
        .total_size()
        .await
        .map_err(|e| AppError::internal_error(e.to_string()))?;
    let available_providers: Vec<String> = state.ai_providers.provider_names();
    let default_provider = state.ai_providers.default_provider_name().map(String::from);
    let active_tasks_count = state
        .project_store
        .count()
        .await
        .map_err(|e| AppError::internal_error(e.to_string()))?;
    let running_jobs_count = count_active_jobs(&state.job_store).await?;

    let available_space_bytes = allocated_space_bytes.saturating_sub(used_space_bytes);

    let info = InfoResult {
        general: GeneralInfo {
            name: state.config.server.name.clone(),
            version: version.to_string(),
        },
        server: ServerInfo {
            allocated_space_bytes,
            used_space_bytes,
            available_space_bytes,
            available_providers,
            default_provider,
            active_tasks_count,
            running_jobs_count,
        },
        limits: LimitsInfo {
            max_photos_per_request: state.config.upload.max_photos_per_request,
            max_photo_size_bytes: state.config.upload.max_photo_size.0,
            max_context_length: state.config.project.max_context_length,
            max_concurrent_jobs: None,
            allowed_photo_types: ALLOWED_MIME_TYPES.iter().map(|s| s.to_string()).collect(),
        },
    };
    Ok(Json(info))
}

async fn count_active_jobs(job_store: &Arc<dyn JobStore>) -> Result<usize, AppError> {
    let jobs = job_store
        .list()
        .await
        .map_err(|e| AppError::internal_error(e.to_string()))?;
    Ok(jobs.iter().filter(|j| !j.is_finished()).count())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::test_utils::fixtures::create_test_state;
    use crate::models::{Job, Photo, Project};
    use uuid::Uuid;

    #[tokio::test]
    async fn test_info_empty_state() {
        let ts = create_test_state().await;

        let result = info(State(ts.state.clone())).await;

        assert!(result.is_ok());
        let Json(info_result) = result.unwrap();

        assert_eq!(info_result.general.name, ts.state.config.server.name);
        assert_eq!(info_result.general.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(
            info_result.server.allocated_space_bytes,
            ts.state.config.storage_max_size()
        );
        assert_eq!(info_result.server.used_space_bytes, 0);
        assert_eq!(
            info_result.server.available_space_bytes,
            ts.state.config.storage_max_size()
        );
        assert_eq!(info_result.server.active_tasks_count, 0);
        assert_eq!(info_result.server.running_jobs_count, 0);
        assert_eq!(
            info_result.server.available_providers,
            vec!["test".to_string()]
        );
        assert_eq!(
            info_result.limits.max_photos_per_request,
            ts.state.config.upload.max_photos_per_request
        );
        assert_eq!(
            info_result.limits.max_photo_size_bytes,
            ts.state.config.upload.max_photo_size.0
        );
        assert_eq!(
            info_result.limits.max_context_length,
            ts.state.config.project.max_context_length
        );
        assert_eq!(info_result.limits.max_concurrent_jobs, None);
        assert_eq!(
            info_result.limits.allowed_photo_types,
            ALLOWED_MIME_TYPES
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn test_info_with_tasks_and_jobs() {
        let ts = create_test_state().await;

        let task1 = Project::new(Uuid::new_v4(), "Task 1".to_string(), "task 1".to_string());
        let task1_id = task1.project_id;
        ts.state.project_store.create(task1).await.unwrap();

        let task2 = Project::new(Uuid::new_v4(), "Task 2".to_string(), "task 2".to_string());
        ts.state.project_store.create(task2).await.unwrap();

        let photo = Photo::new(task1_id, None, "photo1.jpg".to_string(), 5000);
        ts.state.photo_store.create(photo).await.unwrap();

        let job = Job::new(
            task1_id,
            "ollama".to_string(),
            "test".to_string(),
            None,
            vec![],
        );
        ts.state.job_store.create(job).await.unwrap();

        let mut processing_job = Job::new(
            task1_id,
            "ollama".to_string(),
            "test".to_string(),
            None,
            vec![],
        );
        processing_job.start();
        ts.state.job_store.create(processing_job).await.unwrap();

        let mut finished_job = Job::new(
            task1_id,
            "ollama".to_string(),
            "test".to_string(),
            None,
            vec![],
        );
        finished_job.start();
        finished_job.complete();
        ts.state.job_store.create(finished_job).await.unwrap();

        let result = info(State(ts.state.clone())).await;

        assert!(result.is_ok());
        let Json(info_result) = result.unwrap();

        assert_eq!(info_result.server.active_tasks_count, 2);
        assert_eq!(info_result.server.running_jobs_count, 2);
        assert_eq!(info_result.server.used_space_bytes, 5000);
        assert_eq!(
            info_result.server.available_space_bytes,
            ts.state.config.storage_max_size() - 5000
        );
    }

    #[tokio::test]
    async fn test_info_default_provider() {
        let ts = create_test_state().await;

        let result = info(State(ts.state.clone())).await;

        assert!(result.is_ok());
        let Json(info_result) = result.unwrap();

        assert_eq!(
            info_result.server.default_provider,
            Some("test".to_string())
        );
    }
}
