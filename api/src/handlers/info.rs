// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 The Photometoria contributors

use crate::app_state::AppState;
use crate::handlers::app_error::AppError;
use crate::models::info::{GeneralInfo, InfoResult, ServerInfo};
use crate::storage::{JobStore, PhotoStore};
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
    let available_providers: Vec<String> = state.ai_providers.get_names();
    let default_provider = state.ai_providers.default_provider_name().map(String::from);
    let active_tasks_count = state
        .task_store
        .count()
        .await
        .map_err(|e| AppError::internal_error(e.to_string()))?;
    let running_jobs_count = count_active_jobs(&state.job_store).await?;

    let info = InfoResult {
        general: GeneralInfo {
            version: version.to_string(),
        },
        server: ServerInfo {
            allocated_space_bytes,
            used_space_bytes,
            available_providers,
            default_provider,
            active_tasks_count,
            running_jobs_count,
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
    use crate::models::{Job, Photo, Task};

    #[tokio::test]
    async fn test_info_empty_state() {
        let ts = create_test_state().await;

        let result = info(State(ts.state.clone())).await;

        assert!(result.is_ok());
        let Json(info_result) = result.unwrap();

        assert_eq!(info_result.general.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(
            info_result.server.allocated_space_bytes,
            ts.state.config.storage_max_size()
        );
        assert_eq!(info_result.server.used_space_bytes, 0);
        assert_eq!(info_result.server.active_tasks_count, 0);
        assert_eq!(info_result.server.running_jobs_count, 0);
        assert_eq!(
            info_result.server.available_providers,
            vec!["test".to_string()]
        );
    }

    #[tokio::test]
    async fn test_info_with_tasks_and_jobs() {
        let ts = create_test_state().await;

        let task1 = Task::new("task 1".to_string());
        let task1_id = task1.task_id;
        ts.state.task_store.create(task1).await.unwrap();

        let task2 = Task::new("task 2".to_string());
        ts.state.task_store.create(task2).await.unwrap();

        let photo = Photo::new(task1_id, "photo1.jpg".to_string(), 5000);
        ts.state.photo_store.create(photo).await.unwrap();

        let job = Job::new(task1_id, "test".to_string(), vec![]);
        ts.state.job_store.create(job).await.unwrap();

        let mut processing_job = Job::new(task1_id, "test".to_string(), vec![]);
        processing_job.start();
        ts.state.job_store.create(processing_job).await.unwrap();

        let mut finished_job = Job::new(task1_id, "test".to_string(), vec![]);
        finished_job.start();
        finished_job.complete();
        ts.state.job_store.create(finished_job).await.unwrap();

        let result = info(State(ts.state.clone())).await;

        assert!(result.is_ok());
        let Json(info_result) = result.unwrap();

        assert_eq!(info_result.server.active_tasks_count, 2);
        assert_eq!(info_result.server.running_jobs_count, 2);
        assert_eq!(info_result.server.used_space_bytes, 5000);
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
