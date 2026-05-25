// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 The Photometoria contributors

use crate::app_state::AppState;
use crate::handlers::app_error::{AppError, AppPath};
use crate::handlers::project::{check_no_active_jobs, get_existing_project};
use crate::models::{Photo, PhotoListResponse, PhotoResponse, PhotoSummary};
use crate::storage::PhotoStore;
use axum::Json;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use serde::Deserialize;
use std::sync::Arc;
use uuid::Uuid;

/// Query parameters for listing photos in a task.
#[derive(Debug, Deserialize)]
pub struct TaskPhotosQuery {
    /// Filter by client-provided identifier
    pub client_id: Option<String>,
}

pub async fn task_photos(
    State(state): State<AppState>,
    AppPath(task_id): AppPath<Uuid>,
    Query(query): Query<TaskPhotosQuery>,
) -> Result<Json<PhotoListResponse>, AppError> {
    get_existing_project(&state.project_store, task_id).await?;

    match query.client_id {
        Some(client_id) => {
            let photos = state
                .photo_store
                .find_by_client_id(task_id, &client_id)
                .await
                .map_err(|e| AppError::internal_error(e.to_string()))?;
            let count = photos.len();
            let summaries = photos.into_iter().map(PhotoSummary::from).collect();
            Ok(Json(PhotoListResponse {
                photos: summaries,
                count,
            }))
        }
        None => list_task_photo(&state.photo_store, task_id).await,
    }
}

pub async fn get_photo(
    State(state): State<AppState>,
    AppPath(photo_id): AppPath<Uuid>,
) -> Result<Json<PhotoResponse>, AppError> {
    let photo = get_existing_photo(&state.photo_store, photo_id).await?;
    Ok(Json((&photo).into()))
}

pub async fn delete_photo(
    State(state): State<AppState>,
    AppPath(photo_id): AppPath<Uuid>,
) -> Result<StatusCode, AppError> {
    let photo = get_existing_photo(&state.photo_store, photo_id).await?;
    check_no_active_jobs(&state.job_store, photo.task_id).await?;
    match state.photo_store.delete(photo_id).await {
        Ok(_) => Ok(StatusCode::NO_CONTENT),
        Err(error) => Err(AppError::internal_error(error.to_string())),
    }
}

async fn list_task_photo(
    photo_store: &Arc<dyn PhotoStore>,
    task_id: Uuid,
) -> Result<Json<PhotoListResponse>, AppError> {
    match photo_store.list_by_task(task_id).await {
        Ok(photos) => {
            let count = photos.len();
            let summaries = photos.into_iter().map(PhotoSummary::from).collect();
            Ok(Json(PhotoListResponse {
                photos: summaries,
                count,
            }))
        }
        Err(e) => Err(AppError::internal_error(e.to_string())),
    }
}

async fn get_existing_photo(
    photo_store: &Arc<dyn PhotoStore>,
    photo_id: Uuid,
) -> Result<Photo, AppError> {
    match photo_store.get(photo_id).await {
        Ok(Some(photo)) => Ok(photo),
        Ok(None) => Err(AppError::not_found(format!(
            "Photo with id {} not found",
            photo_id
        ))),
        Err(e) => Err(AppError::internal_error(e.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::test_utils::fixtures::{create_test_state, test_catalog_id};
    use crate::models::{Photo, Project};
    use uuid::Uuid;

    // ========================================================================
    // Tests for task_photos handler
    // ========================================================================

    #[tokio::test]
    async fn test_task_photos_empty() {
        let ts = create_test_state().await;

        // Create a task without photos
        let task = Project::new(
            test_catalog_id(),
            "Test task".to_string(),
            "test task".to_string(),
        );
        let task_id = task.project_id;
        ts.state.project_store.create(task).await.unwrap();

        let result = task_photos(
            State(ts.state.clone()),
            AppPath(task_id),
            Query(TaskPhotosQuery { client_id: None }),
        )
        .await;

        assert!(result.is_ok());
        let Json(photo_list) = result.unwrap();
        assert_eq!(photo_list.photos.len(), 0);
        assert_eq!(photo_list.count, 0);
    }

    #[tokio::test]
    async fn test_task_photos_with_photos() {
        let ts = create_test_state().await;

        // Create a task
        let task = Project::new(
            test_catalog_id(),
            "Test task with photos".to_string(),
            "test task with photos".to_string(),
        );
        let task_id = task.project_id;
        ts.state.project_store.create(task).await.unwrap();

        // Create multiple photos
        let photo1 = Photo::new(task_id, None, "photo1.jpg".to_string(), 1000);
        let photo2 = Photo::new(task_id, None, "photo2.jpg".to_string(), 2000);
        let photo3 = Photo::new(task_id, None, "photo3.jpg".to_string(), 3000);

        let photo1_id = photo1.photo_id;
        let photo2_id = photo2.photo_id;
        let photo3_id = photo3.photo_id;

        ts.state.photo_store.create(photo1).await.unwrap();
        ts.state.photo_store.create(photo2).await.unwrap();
        ts.state.photo_store.create(photo3).await.unwrap();

        let result = task_photos(
            State(ts.state.clone()),
            AppPath(task_id),
            Query(TaskPhotosQuery { client_id: None }),
        )
        .await;

        assert!(result.is_ok());
        let Json(photo_list) = result.unwrap();
        assert_eq!(photo_list.count, 3);
        assert_eq!(photo_list.photos.len(), 3);
        let ids: Vec<Uuid> = photo_list.photos.iter().map(|p| p.photo_id).collect();
        assert!(ids.contains(&photo1_id));
        assert!(ids.contains(&photo2_id));
        assert!(ids.contains(&photo3_id));
    }

    #[tokio::test]
    async fn test_task_photos_task_not_found() {
        let ts = create_test_state().await;
        let nonexistent_id = Uuid::new_v4();

        let result = task_photos(
            State(ts.state.clone()),
            AppPath(nonexistent_id),
            Query(TaskPhotosQuery { client_id: None }),
        )
        .await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.body.error, "not_found");
        assert!(error.body.message.contains(&nonexistent_id.to_string()));
    }

    // ========================================================================
    // Tests for get_photo handler
    // ========================================================================

    #[tokio::test]
    async fn test_get_photo_found() {
        let ts = create_test_state().await;

        // Create task and photo
        let task = Project::new(
            test_catalog_id(),
            "Test task".to_string(),
            "test task".to_string(),
        );
        let task_id = task.project_id;
        ts.state.project_store.create(task).await.unwrap();

        let photo = Photo::new(task_id, None, "test.jpg".to_string(), 1_234_567);
        let photo_id = photo.photo_id;
        let uploaded_at = photo.uploaded_at;
        ts.state.photo_store.create(photo).await.unwrap();

        let result = get_photo(State(ts.state.clone()), AppPath(photo_id)).await;

        assert!(result.is_ok());
        let Json(photo_response) = result.unwrap();
        assert_eq!(photo_response.photo_id, photo_id);
        assert_eq!(photo_response.task_id, task_id);
        assert_eq!(photo_response.filename, "test.jpg");
        assert_eq!(photo_response.size_bytes, 1_234_567);
        assert_eq!(photo_response.uploaded_at, uploaded_at);
    }

    #[tokio::test]
    async fn test_get_photo_not_found() {
        let ts = create_test_state().await;
        let nonexistent_id = Uuid::new_v4();

        let result = get_photo(State(ts.state.clone()), AppPath(nonexistent_id)).await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.body.error, "not_found");
        assert!(error.body.message.contains(&nonexistent_id.to_string()));
    }

    // ========================================================================
    // Tests for delete_photo handler
    // ========================================================================

    #[tokio::test]
    async fn test_delete_photo_success() {
        let ts = create_test_state().await;

        // Create task and photo
        let task = Project::new(
            test_catalog_id(),
            "Test task".to_string(),
            "test task".to_string(),
        );
        let task_id = task.project_id;
        ts.state.project_store.create(task).await.unwrap();

        let photo = Photo::new(task_id, None, "to_delete.jpg".to_string(), 5000);
        let photo_id = photo.photo_id;
        ts.state.photo_store.create(photo).await.unwrap();

        let result = delete_photo(State(ts.state.clone()), AppPath(photo_id)).await;

        assert_eq!(result, Ok(StatusCode::NO_CONTENT));

        // Verify photo is deleted
        let exists = ts.state.photo_store.exists(photo_id).await.unwrap();
        assert!(!exists);
    }

    #[tokio::test]
    async fn test_delete_photo_not_found() {
        let ts = create_test_state().await;
        let nonexistent_id = Uuid::new_v4();

        let result = delete_photo(State(ts.state.clone()), AppPath(nonexistent_id)).await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.body.error, "not_found");
        assert!(error.body.message.contains(&nonexistent_id.to_string()));
    }

    #[tokio::test]
    async fn test_delete_photo_blocked_by_queued_job() {
        let ts = create_test_state().await;

        let task =
            crate::models::Project::new(test_catalog_id(), "Task".to_string(), "task".to_string());
        let task_id = task.project_id;
        ts.state.project_store.create(task).await.unwrap();

        let photo = Photo::new(task_id, None, "photo.jpg".to_string(), 1000);
        let photo_id = photo.photo_id;
        ts.state.photo_store.create(photo).await.unwrap();

        let job = crate::models::Job::new(
            task_id,
            "ollama".to_string(),
            "llava".to_string(),
            None,
            vec![photo_id],
        );
        ts.state.job_store.create(job).await.unwrap();

        let result = delete_photo(State(ts.state.clone()), AppPath(photo_id)).await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().body.error, "job_active");
    }

    #[tokio::test]
    async fn test_delete_photo_blocked_by_processing_job() {
        let ts = create_test_state().await;

        let task =
            crate::models::Project::new(test_catalog_id(), "Task".to_string(), "task".to_string());
        let task_id = task.project_id;
        ts.state.project_store.create(task).await.unwrap();

        let photo = Photo::new(task_id, None, "photo.jpg".to_string(), 1000);
        let photo_id = photo.photo_id;
        ts.state.photo_store.create(photo).await.unwrap();

        let mut job = crate::models::Job::new(
            task_id,
            "ollama".to_string(),
            "llava".to_string(),
            None,
            vec![photo_id],
        );
        job.start();
        ts.state.job_store.create(job).await.unwrap();

        let result = delete_photo(State(ts.state.clone()), AppPath(photo_id)).await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().body.error, "job_active");
    }

    #[tokio::test]
    async fn test_delete_photo_allowed_when_job_completed() {
        let ts = create_test_state().await;

        let task =
            crate::models::Project::new(test_catalog_id(), "Task".to_string(), "task".to_string());
        let task_id = task.project_id;
        ts.state.project_store.create(task).await.unwrap();

        let photo = Photo::new(task_id, None, "photo.jpg".to_string(), 1000);
        let photo_id = photo.photo_id;
        ts.state.photo_store.create(photo).await.unwrap();

        let mut job = crate::models::Job::new(
            task_id,
            "ollama".to_string(),
            "llava".to_string(),
            None,
            vec![photo_id],
        );
        job.start();
        job.complete();
        ts.state.job_store.create(job).await.unwrap();

        let result = delete_photo(State(ts.state.clone()), AppPath(photo_id)).await;

        assert_eq!(result, Ok(StatusCode::NO_CONTENT));
    }

    // ========================================================================
    // Tests for client_id support
    // ========================================================================

    #[tokio::test]
    async fn test_get_photo_returns_client_id() {
        let ts = create_test_state().await;

        let task = Project::new(
            test_catalog_id(),
            "Test task".to_string(),
            "test task".to_string(),
        );
        let task_id = task.project_id;
        ts.state.project_store.create(task).await.unwrap();

        let photo = Photo::new(
            task_id,
            Some("lr:42".to_string()),
            "test.jpg".to_string(),
            1000,
        );
        let photo_id = photo.photo_id;
        ts.state.photo_store.create(photo).await.unwrap();

        let result = get_photo(State(ts.state.clone()), AppPath(photo_id)).await;

        assert!(result.is_ok());
        let Json(response) = result.unwrap();
        assert_eq!(response.client_id, Some("lr:42".to_string()));
    }

    #[tokio::test]
    async fn test_get_photo_returns_none_client_id() {
        let ts = create_test_state().await;

        let task = Project::new(
            test_catalog_id(),
            "Test task".to_string(),
            "test task".to_string(),
        );
        let task_id = task.project_id;
        ts.state.project_store.create(task).await.unwrap();

        let photo = Photo::new(task_id, None, "test.jpg".to_string(), 1000);
        let photo_id = photo.photo_id;
        ts.state.photo_store.create(photo).await.unwrap();

        let result = get_photo(State(ts.state.clone()), AppPath(photo_id)).await;

        assert!(result.is_ok());
        let Json(response) = result.unwrap();
        assert!(response.client_id.is_none());
    }

    #[tokio::test]
    async fn test_task_photos_filter_by_client_id() {
        let ts = create_test_state().await;

        let task = Project::new(
            test_catalog_id(),
            "Test task".to_string(),
            "test task".to_string(),
        );
        let task_id = task.project_id;
        ts.state.project_store.create(task).await.unwrap();

        let photo1 = Photo::new(
            task_id,
            Some("lr:100".to_string()),
            "photo1.jpg".to_string(),
            1000,
        );
        let photo2 = Photo::new(
            task_id,
            Some("lr:200".to_string()),
            "photo2.jpg".to_string(),
            2000,
        );
        let photo3 = Photo::new(task_id, None, "photo3.jpg".to_string(), 3000);
        let photo1_id = photo1.photo_id;

        ts.state.photo_store.create(photo1).await.unwrap();
        ts.state.photo_store.create(photo2).await.unwrap();
        ts.state.photo_store.create(photo3).await.unwrap();

        let result = task_photos(
            State(ts.state.clone()),
            AppPath(task_id),
            Query(TaskPhotosQuery {
                client_id: Some("lr:100".to_string()),
            }),
        )
        .await;

        assert!(result.is_ok());
        let Json(photo_list) = result.unwrap();
        assert_eq!(photo_list.count, 1);
        assert_eq!(photo_list.photos.len(), 1);
        assert_eq!(photo_list.photos[0].photo_id, photo1_id);
        assert_eq!(photo_list.photos[0].client_id, Some("lr:100".to_string()));
    }

    #[tokio::test]
    async fn test_task_photos_filter_by_client_id_no_match() {
        let ts = create_test_state().await;

        let task = Project::new(
            test_catalog_id(),
            "Test task".to_string(),
            "test task".to_string(),
        );
        let task_id = task.project_id;
        ts.state.project_store.create(task).await.unwrap();

        let photo = Photo::new(
            task_id,
            Some("lr:100".to_string()),
            "photo.jpg".to_string(),
            1000,
        );
        ts.state.photo_store.create(photo).await.unwrap();

        let result = task_photos(
            State(ts.state.clone()),
            AppPath(task_id),
            Query(TaskPhotosQuery {
                client_id: Some("lr:999".to_string()),
            }),
        )
        .await;

        assert!(result.is_ok());
        let Json(photo_list) = result.unwrap();
        assert_eq!(photo_list.count, 0);
        assert!(photo_list.photos.is_empty());
    }
}
