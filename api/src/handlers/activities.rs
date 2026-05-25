// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 The Photometoria contributors

use crate::app_state::AppState;
use crate::handlers::app_error::{AppError, AppPath};
use crate::handlers::project::get_existing_project;
use crate::models::{
    Activity, ActivityDetailResponse, ActivityResponse, ActivityResultsResponse, ActivitySummary,
    CreateActivityRequest, Photo, RetryActivityResponse,
};
use crate::services::ai::AIProviderError;
use crate::storage::ActivityStore;
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use std::collections::HashSet;
use std::sync::Arc;
use uuid::Uuid;

/// Handler for POST /api/tasks/{task_id}/jobs
///
/// Creates a new activity for processing photos with AI analysis.
pub async fn create_activity(
    State(state): State<AppState>,
    AppPath(task_id): AppPath<Uuid>,
    Json(request): Json<CreateActivityRequest>,
) -> Result<(StatusCode, Json<ActivityResponse>), AppError> {
    get_existing_project(&state.project_store, task_id).await?;

    if !state.ai_providers.contains(&request.provider) {
        return Err(AppError::bad_request(
            "invalid_provider",
            format!("Provider '{}' is not configured", request.provider),
        ));
    }

    state
        .ai_providers
        .check_model_available(&request.provider, &request.model)
        .await
        .map_err(|e| match e {
            AIProviderError::ModelNotFound { .. } => AppError::bad_request(
                "invalid_model",
                format!(
                    "Model '{}' is not configured for provider '{}'",
                    request.model, request.provider
                ),
            ),
            AIProviderError::ModelNotAvailable { .. } => AppError::bad_request(
                "model_not_available",
                format!(
                    "Model '{}' is not available in the AI provider",
                    request.model
                ),
            ),
            AIProviderError::Unavailable { .. } | AIProviderError::Timeout { .. } => {
                AppError::service_unavailable(format!("AI provider is not available: {}", e))
            }
            e => AppError::internal_error(e.to_string()),
        })?;

    match state.photo_store.list_by_task(task_id).await {
        Ok(photos) => {
            let photo_ids = activity_photo_ids(task_id, photos, request.photo_ids)?;
            if photo_ids.is_empty() {
                return Err(AppError::bad_request(
                    "no_photos",
                    "Task must have at least one photo to create a job",
                ));
            }
            let language = request
                .language
                .or_else(|| state.config.ai.default_language.clone());
            let activity = Activity::new(
                task_id,
                request.provider,
                request.model,
                language,
                photo_ids,
            );
            let activity_response = add_activity_to_store(&state.activity_store, activity).await?;
            Ok((StatusCode::CREATED, Json(activity_response)))
        }
        Err(e) => Err(AppError::internal_error(e.to_string())),
    }
}

fn activity_photo_ids(
    task_id: Uuid,
    photos: Vec<Photo>,
    request_photo_ids: Option<Vec<Uuid>>,
) -> Result<Vec<Uuid>, AppError> {
    let photo_ids = match request_photo_ids {
        Some(photo_ids) => {
            let task_photo_ids = photos.iter().map(|p| &p.photo_id).collect::<HashSet<_>>();
            for photo_id in photo_ids.clone() {
                if !task_photo_ids.contains(&photo_id) {
                    return Err(AppError::bad_request(
                        "invalid_parameter",
                        format!(
                            "photo with photo_id '{}' does not exists in task '{}'",
                            photo_id, task_id
                        ),
                    ));
                }
            }
            photo_ids
        }
        None => photos.iter().map(|p| p.photo_id).collect(),
    };
    Ok(photo_ids)
}

async fn add_activity_to_store(
    activity_store: &Arc<dyn ActivityStore>,
    activity: Activity,
) -> Result<ActivityResponse, AppError> {
    match activity_store.create(activity.clone()).await {
        Ok(activity) => Ok(activity.into()),
        Err(e) => Err(AppError::internal_error(e.to_string())),
    }
}

/// Helper function to retrieve an activity and handle errors.
///
/// Returns the activity if found, or an appropriate AppError otherwise.
async fn get_existing_activity(
    activity_store: &Arc<dyn ActivityStore>,
    activity_id: Uuid,
) -> Result<Activity, AppError> {
    match activity_store.get(activity_id).await {
        Ok(Some(activity)) => Ok(activity),
        Ok(None) => Err(AppError::activity_not_found(activity_id)),
        Err(e) => Err(AppError::internal_error(e.to_string())),
    }
}

/// Handler for GET /api/tasks/{task_id}/jobs
///
/// Lists all activities belonging to a specific task with summary information.
pub async fn list_project_activities(
    State(state): State<AppState>,
    AppPath(task_id): AppPath<Uuid>,
) -> Result<Json<Vec<ActivitySummary>>, AppError> {
    get_existing_project(&state.project_store, task_id).await?;

    match state.activity_store.list_by_task(task_id).await {
        Ok(activities) => {
            let summaries: Vec<ActivitySummary> =
                activities.iter().map(|activity| activity.into()).collect();
            Ok(Json(summaries))
        }
        Err(e) => Err(AppError::internal_error(e.to_string())),
    }
}

/// Handler for GET /api/jobs
///
/// Lists all activities with summary information.
pub async fn list_activities(
    State(state): State<AppState>,
) -> Result<Json<Vec<ActivitySummary>>, AppError> {
    match state.activity_store.list().await {
        Ok(activities) => {
            let summaries: Vec<ActivitySummary> =
                activities.iter().map(|activity| activity.into()).collect();
            Ok(Json(summaries))
        }
        Err(e) => Err(AppError::internal_error(e.to_string())),
    }
}

/// Handler for GET /api/jobs/{job_id}
///
/// Retrieves detailed information about a specific activity, including progress if processing.
pub async fn get_activity(
    State(state): State<AppState>,
    AppPath(activity_id): AppPath<Uuid>,
) -> Result<Json<ActivityDetailResponse>, AppError> {
    let activity = get_existing_activity(&state.activity_store, activity_id).await?;
    let response: ActivityDetailResponse = activity.into();
    Ok(Json(response))
}

/// Handler for GET /api/jobs/{job_id}/results
///
/// Retrieves AI analysis results for all processed photos in an activity.
pub async fn get_activity_results(
    State(state): State<AppState>,
    AppPath(activity_id): AppPath<Uuid>,
) -> Result<Json<ActivityResultsResponse>, AppError> {
    let activity = get_existing_activity(&state.activity_store, activity_id).await?;
    let response: ActivityResultsResponse = activity.into();
    Ok(Json(response))
}

/// Handler for POST /api/jobs/{job_id}/cancel
///
/// Cancels an active activity. Removes any pending photos from the worker buffer
/// and marks the activity as Cancelled. Photos already being processed by a worker
/// will still complete, but their results won't be saved.
pub async fn cancel_activity(
    State(state): State<AppState>,
    AppPath(activity_id): AppPath<Uuid>,
) -> Result<Json<ActivityResponse>, AppError> {
    let mut activity = get_existing_activity(&state.activity_store, activity_id).await?;

    if activity.is_finished() {
        return Err(AppError::conflict(
            "job_already_finished",
            format!(
                "Cannot cancel job '{}': it has already finished with status '{}'",
                activity_id, activity.status
            ),
        ));
    }

    // Remove any photos still waiting in the worker buffer
    {
        let pool = state.worker_pool.lock().await;
        pool.cancel_activity(activity_id).await;
    }

    // Mark activity as cancelled and persist
    activity.cancel();
    match state.activity_store.update(activity).await {
        Ok(updated_activity) => Ok(Json((&updated_activity).into())),
        Err(e) => Err(AppError::internal_error(e.to_string())),
    }
}

/// Handler for POST /api/jobs/{job_id}/retry
///
/// Creates a new activity to retry unprocessed or failed photos from an existing activity.
/// Includes both photos that failed during processing and photos that were never
/// processed (e.g. because the original activity was cancelled).
pub async fn retry_activity(
    State(state): State<AppState>,
    AppPath(activity_id): AppPath<Uuid>,
) -> Result<Json<RetryActivityResponse>, AppError> {
    let original_activity = get_existing_activity(&state.activity_store, activity_id).await?;

    // Validate activity is finished
    if !original_activity.is_finished() {
        return Err(AppError::conflict(
            "job_in_progress",
            format!(
                "Cannot retry job '{}' while it is still processing",
                activity_id
            ),
        ));
    }

    // Get failed + unprocessed photo IDs
    let retriable_photo_ids = original_activity.retriable_photo_ids();
    if retriable_photo_ids.is_empty() {
        return Err(AppError::bad_request(
            "no_failed_photos",
            format!(
                "Job '{}' has no failed or unprocessed photos to retry",
                activity_id
            ),
        ));
    }

    // Create new activity with retriable photos, preserving the provider and language from the original
    let new_activity = Activity::new(
        original_activity.project_id,
        original_activity.provider.clone(),
        original_activity.model.clone(),
        original_activity.language.clone(),
        retriable_photo_ids,
    );

    // Store the new activity
    match state.activity_store.create(new_activity.clone()).await {
        Ok(created_activity) => Ok(Json(RetryActivityResponse {
            activity_id: created_activity.activity_id,
            project_id: created_activity.project_id,
            status: created_activity.status,
            provider: created_activity.provider,
            model: created_activity.model,
            language: created_activity.language,
            photo_count: created_activity.photo_ids.len(),
            created_at: created_activity.created_at,
            parent_activity_id: activity_id,
        })),
        Err(e) => Err(AppError::internal_error(e.to_string())),
    }
}

/// Handler for DELETE /api/jobs/{job_id}
///
/// Deletes an activity. The activity must be in a terminal state (Completed, Failed, Cancelled).
/// Returns 409 Conflict if the activity is still active (Queued or Processing).
pub async fn delete_activity(
    State(state): State<AppState>,
    AppPath(activity_id): AppPath<Uuid>,
) -> Result<axum::http::StatusCode, AppError> {
    let activity = get_existing_activity(&state.activity_store, activity_id).await?;

    if !activity.is_finished() {
        return Err(AppError::conflict(
            "job_not_finished",
            format!("Cannot delete job '{}': it is still active", activity_id),
        ));
    }

    match state.activity_store.delete(activity_id).await {
        Ok(_) => Ok(axum::http::StatusCode::NO_CONTENT),
        Err(e) => Err(AppError::internal_error(e.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::test_utils::fixtures::{
        create_test_state, create_test_state_model_not_available,
        create_test_state_provider_unavailable, test_catalog_id,
    };
    use crate::models::{ActivityStatus, Photo, Project};
    use uuid::Uuid;

    // ========================================================================
    // Tests for create_activity handler
    // ========================================================================

    #[tokio::test]
    async fn test_create_activity_all_photos() {
        let ts = create_test_state().await;

        // Create task with photos
        let task = Project::new(
            test_catalog_id(),
            "Test task".to_string(),
            "test task".to_string(),
        );
        let task_id = task.project_id;
        ts.state.project_store.create(task).await.unwrap();

        let photo1 = Photo::new(task_id, None, "photo1.jpg".to_string(), 1000);
        let photo2 = Photo::new(task_id, None, "photo2.jpg".to_string(), 2000);
        let photo1_id = photo1.photo_id;
        let photo2_id = photo2.photo_id;

        ts.state.photo_store.create(photo1).await.unwrap();
        ts.state.photo_store.create(photo2).await.unwrap();

        // Create activity with all photos (photo_ids = None)
        let request = CreateActivityRequest {
            provider: "test".to_string(),
            model: "qwen3-vl:8b".to_string(),
            language: None,
            photo_ids: None,
        };

        let result =
            create_activity(State(ts.state.clone()), AppPath(task_id), Json(request)).await;

        assert!(result.is_ok());
        let (status, Json(activity_response)) = result.unwrap();
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(activity_response.project_id, task_id);
        assert_eq!(activity_response.model, "qwen3-vl:8b");
        assert_eq!(activity_response.photo_count, 2);
        assert_eq!(activity_response.status, ActivityStatus::Queued);

        // Verify the activity was stored and contains all photos
        let stored_activity = ts
            .state
            .activity_store
            .get(activity_response.activity_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored_activity.photo_ids.len(), 2);
        assert!(stored_activity.photo_ids.contains(&photo1_id));
        assert!(stored_activity.photo_ids.contains(&photo2_id));
    }

    #[tokio::test]
    async fn test_create_activity_specific_photos() {
        let ts = create_test_state().await;

        // Create task with photos
        let task = Project::new(
            test_catalog_id(),
            "Test task".to_string(),
            "test task".to_string(),
        );
        let task_id = task.project_id;
        ts.state.project_store.create(task).await.unwrap();

        let photo1 = Photo::new(task_id, None, "photo1.jpg".to_string(), 1000);
        let photo2 = Photo::new(task_id, None, "photo2.jpg".to_string(), 2000);
        let photo3 = Photo::new(task_id, None, "photo3.jpg".to_string(), 3000);
        let photo1_id = photo1.photo_id;
        let photo2_id = photo2.photo_id;

        ts.state.photo_store.create(photo1).await.unwrap();
        ts.state.photo_store.create(photo2).await.unwrap();
        ts.state.photo_store.create(photo3).await.unwrap();

        // Create activity with only photo1 and photo2
        let request = CreateActivityRequest {
            provider: "test".to_string(),
            model: "llava".to_string(),
            language: None,
            photo_ids: Some(vec![photo1_id, photo2_id]),
        };

        let result =
            create_activity(State(ts.state.clone()), AppPath(task_id), Json(request)).await;

        assert!(result.is_ok());
        let (status, Json(activity_response)) = result.unwrap();
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(activity_response.project_id, task_id);
        assert_eq!(activity_response.model, "llava");
        assert_eq!(activity_response.photo_count, 2);

        // Verify the activity contains only the specified photos
        let stored_activity = ts
            .state
            .activity_store
            .get(activity_response.activity_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored_activity.photo_ids.len(), 2);
        assert!(stored_activity.photo_ids.contains(&photo1_id));
        assert!(stored_activity.photo_ids.contains(&photo2_id));
    }

    #[tokio::test]
    async fn test_create_activity_invalid_provider() {
        let ts = create_test_state().await;

        let task = Project::new(
            test_catalog_id(),
            "Test task".to_string(),
            "test task".to_string(),
        );
        let task_id = task.project_id;
        ts.state.project_store.create(task).await.unwrap();

        let request = CreateActivityRequest {
            provider: "nonexistent-provider".to_string(),
            model: "qwen3-vl:8b".to_string(),
            language: None,
            photo_ids: None,
        };

        let result =
            create_activity(State(ts.state.clone()), AppPath(task_id), Json(request)).await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.status, axum::http::StatusCode::BAD_REQUEST);
        assert_eq!(error.body.error, "invalid_provider");
        assert!(error.body.message.contains("nonexistent-provider"));
    }

    #[tokio::test]
    async fn test_create_activity_invalid_model() {
        let ts = create_test_state().await;

        let task = Project::new(
            test_catalog_id(),
            "Test task".to_string(),
            "test task".to_string(),
        );
        let task_id = task.project_id;
        ts.state.project_store.create(task).await.unwrap();

        let request = CreateActivityRequest {
            provider: "test".to_string(),
            model: "nonexistent-model".to_string(),
            language: None,
            photo_ids: None,
        };

        let result =
            create_activity(State(ts.state.clone()), AppPath(task_id), Json(request)).await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.body.error, "invalid_model");
        assert!(error.body.message.contains("nonexistent-model"));
    }

    #[tokio::test]
    async fn test_create_activity_model_not_available() {
        let ts = create_test_state_model_not_available().await;

        let task = Project::new(
            test_catalog_id(),
            "Test task".to_string(),
            "test task".to_string(),
        );
        let task_id = task.project_id;
        ts.state.project_store.create(task).await.unwrap();

        let photo = Photo::new(task_id, None, "photo.jpg".to_string(), 1000);
        ts.state.photo_store.create(photo).await.unwrap();

        let request = CreateActivityRequest {
            provider: "test".to_string(),
            model: "qwen3-vl:8b".to_string(),
            language: None,
            photo_ids: None,
        };

        let result =
            create_activity(State(ts.state.clone()), AppPath(task_id), Json(request)).await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.status, axum::http::StatusCode::BAD_REQUEST);
        assert_eq!(error.body.error, "model_not_available");
        assert!(error.body.message.contains("qwen3-vl:8b"));
    }

    #[tokio::test]
    async fn test_create_activity_provider_unavailable() {
        let ts = create_test_state_provider_unavailable().await;

        let task = Project::new(
            test_catalog_id(),
            "Test task".to_string(),
            "test task".to_string(),
        );
        let task_id = task.project_id;
        ts.state.project_store.create(task).await.unwrap();

        let photo = Photo::new(task_id, None, "photo.jpg".to_string(), 1000);
        ts.state.photo_store.create(photo).await.unwrap();

        let request = CreateActivityRequest {
            provider: "test".to_string(),
            model: "qwen3-vl:8b".to_string(),
            language: None,
            photo_ids: None,
        };

        let result =
            create_activity(State(ts.state.clone()), AppPath(task_id), Json(request)).await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.status, axum::http::StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(error.body.error, "service_unavailable");
    }

    #[tokio::test]
    async fn test_create_activity_task_not_found() {
        let ts = create_test_state().await;
        let nonexistent_task_id = Uuid::new_v4();

        let request = CreateActivityRequest {
            provider: "test".to_string(),
            model: "qwen3-vl:8b".to_string(),
            language: None,
            photo_ids: None,
        };

        let result = create_activity(
            State(ts.state.clone()),
            AppPath(nonexistent_task_id),
            Json(request),
        )
        .await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.body.error, "not_found");
        assert!(
            error
                .body
                .message
                .contains(&nonexistent_task_id.to_string())
        );
    }

    #[tokio::test]
    async fn test_create_activity_invalid_photo_id() {
        let ts = create_test_state().await;

        // Create task with one photo
        let task = Project::new(
            test_catalog_id(),
            "Test task".to_string(),
            "test task".to_string(),
        );
        let task_id = task.project_id;
        ts.state.project_store.create(task).await.unwrap();

        let photo1 = Photo::new(task_id, None, "photo1.jpg".to_string(), 1000);
        ts.state.photo_store.create(photo1).await.unwrap();

        // Try to create activity with a photo_id that doesn't belong to this task
        let invalid_photo_id = Uuid::new_v4();
        let request = CreateActivityRequest {
            provider: "test".to_string(),
            model: "qwen3-vl:8b".to_string(),
            language: None,
            photo_ids: Some(vec![invalid_photo_id]),
        };

        let result =
            create_activity(State(ts.state.clone()), AppPath(task_id), Json(request)).await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.body.error, "invalid_parameter");
        assert!(error.body.message.contains(&invalid_photo_id.to_string()));
        assert!(error.body.message.contains(&task_id.to_string()));
    }

    #[tokio::test]
    async fn test_create_activity_empty_task() {
        let ts = create_test_state().await;

        // Create task without photos
        let task = Project::new(
            test_catalog_id(),
            "Empty task".to_string(),
            "empty task".to_string(),
        );
        let task_id = task.project_id;
        ts.state.project_store.create(task).await.unwrap();

        // Create activity for empty task
        let request = CreateActivityRequest {
            provider: "test".to_string(),
            model: "qwen3-vl:8b".to_string(),
            language: None,
            photo_ids: None,
        };

        let result =
            create_activity(State(ts.state.clone()), AppPath(task_id), Json(request)).await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.body.error, "no_photos");
    }

    #[tokio::test]
    async fn test_create_activity_explicit_empty_photo_ids() {
        let ts = create_test_state().await;

        let task = Project::new(
            test_catalog_id(),
            "Task with photos".to_string(),
            "task with photos".to_string(),
        );
        let task_id = task.project_id;
        ts.state.project_store.create(task).await.unwrap();

        let photo = Photo::new(task_id, None, "photo.jpg".to_string(), 1000);
        ts.state.photo_store.create(photo).await.unwrap();

        // Explicitly pass an empty photo_ids list
        let request = CreateActivityRequest {
            provider: "test".to_string(),
            model: "qwen3-vl:8b".to_string(),
            language: None,
            photo_ids: Some(vec![]),
        };

        let result =
            create_activity(State(ts.state.clone()), AppPath(task_id), Json(request)).await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.body.error, "no_photos");
    }

    // ========================================================================
    // Tests for list_activities handler
    // ========================================================================

    #[tokio::test]
    async fn test_list_activities_empty() {
        let ts = create_test_state().await;

        let result = list_activities(State(ts.state.clone())).await;

        assert!(result.is_ok());
        let Json(activities) = result.unwrap();
        assert!(activities.is_empty());
    }

    #[tokio::test]
    async fn test_list_activities_multiple() {
        let ts = create_test_state().await;

        // Create task with photos
        let task = Project::new(
            test_catalog_id(),
            "Test task".to_string(),
            "test task".to_string(),
        );
        let task_id = task.project_id;
        ts.state.project_store.create(task).await.unwrap();

        let photo1 = Photo::new(task_id, None, "photo1.jpg".to_string(), 1000);
        ts.state.photo_store.create(photo1).await.unwrap();

        // Create multiple activities
        let activity1 = Activity::new(
            task_id,
            "ollama".to_string(),
            "qwen3-vl:8b".to_string(),
            None,
            vec![],
        );
        let activity2 = Activity::new(
            task_id,
            "ollama".to_string(),
            "llava".to_string(),
            None,
            vec![],
        );
        ts.state
            .activity_store
            .create(activity1.clone())
            .await
            .unwrap();
        ts.state
            .activity_store
            .create(activity2.clone())
            .await
            .unwrap();

        let result = list_activities(State(ts.state.clone())).await;

        assert!(result.is_ok());
        let Json(activities) = result.unwrap();
        assert_eq!(activities.len(), 2);
        assert_eq!(activities[0].project_id, task_id);
        assert_eq!(activities[1].project_id, task_id);
        assert_eq!(activities[0].status, ActivityStatus::Queued);
        assert_eq!(activities[1].status, ActivityStatus::Queued);
    }

    #[tokio::test]
    async fn test_list_activities_mixed_statuses() {
        let ts = create_test_state().await;

        // Create task
        let task = Project::new(
            test_catalog_id(),
            "Test task".to_string(),
            "test task".to_string(),
        );
        let task_id = task.project_id;
        ts.state.project_store.create(task).await.unwrap();

        // Create activities with different statuses
        let mut activity1 = Activity::new(
            task_id,
            "ollama".to_string(),
            "qwen3-vl:8b".to_string(),
            None,
            vec![],
        );
        activity1.start();
        let mut activity2 = Activity::new(
            task_id,
            "ollama".to_string(),
            "llava".to_string(),
            None,
            vec![],
        );
        activity2.start();
        activity2.complete();

        ts.state
            .activity_store
            .create(activity1.clone())
            .await
            .unwrap();
        ts.state
            .activity_store
            .create(activity2.clone())
            .await
            .unwrap();

        let result = list_activities(State(ts.state.clone())).await;

        assert!(result.is_ok());
        let Json(activities) = result.unwrap();
        assert_eq!(activities.len(), 2);
        // Check that we have both processing and completed
        let statuses: Vec<ActivityStatus> = activities.iter().map(|a| a.status).collect();
        assert!(statuses.contains(&ActivityStatus::Processing));
        assert!(statuses.contains(&ActivityStatus::Completed));
    }

    // ========================================================================
    // Tests for get_activity handler
    // ========================================================================

    #[tokio::test]
    async fn test_get_activity_found() {
        let ts = create_test_state().await;

        // Create task and activity
        let task = Project::new(
            test_catalog_id(),
            "Test task".to_string(),
            "test task".to_string(),
        );
        let task_id = task.project_id;
        ts.state.project_store.create(task).await.unwrap();

        let activity = Activity::new(
            task_id,
            "ollama".to_string(),
            "qwen3-vl:8b".to_string(),
            None,
            vec![Uuid::new_v4(), Uuid::new_v4()],
        );
        let activity_id = activity.activity_id;
        ts.state.activity_store.create(activity).await.unwrap();

        let result = get_activity(State(ts.state.clone()), AppPath(activity_id)).await;

        assert!(result.is_ok());
        let Json(response) = result.unwrap();
        assert_eq!(response.activity_id, activity_id);
        assert_eq!(response.project_id, task_id);
        assert_eq!(response.status, ActivityStatus::Queued);
        assert_eq!(response.photo_count, 2);
        assert!(response.progress.is_none()); // No progress when queued
    }

    #[tokio::test]
    async fn test_get_activity_with_progress() {
        let ts = create_test_state().await;

        // Create task and activity
        let task = Project::new(
            test_catalog_id(),
            "Test task".to_string(),
            "test task".to_string(),
        );
        let task_id = task.project_id;
        ts.state.project_store.create(task).await.unwrap();

        let mut activity = Activity::new(
            task_id,
            "ollama".to_string(),
            "qwen3-vl:8b".to_string(),
            None,
            vec![Uuid::new_v4(), Uuid::new_v4()],
        );
        activity.start();
        let activity_id = activity.activity_id;
        ts.state.activity_store.create(activity).await.unwrap();

        let result = get_activity(State(ts.state.clone()), AppPath(activity_id)).await;

        assert!(result.is_ok());
        let Json(response) = result.unwrap();
        assert_eq!(response.status, ActivityStatus::Processing);
        assert!(response.progress.is_some()); // Should have progress when processing
        let progress = response.progress.unwrap();
        assert_eq!(progress.completed, 0);
        assert_eq!(progress.failed, 0);
        assert_eq!(progress.remaining, 2);
    }

    #[tokio::test]
    async fn test_get_activity_not_found() {
        let ts = create_test_state().await;
        let nonexistent_activity_id = Uuid::new_v4();

        let result = get_activity(State(ts.state.clone()), AppPath(nonexistent_activity_id)).await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.body.error, "not_found");
        assert!(
            error
                .body
                .message
                .contains(&nonexistent_activity_id.to_string())
        );
    }

    // ========================================================================
    // Tests for get_activity_results handler
    // ========================================================================

    #[tokio::test]
    async fn test_get_activity_results_empty() {
        let ts = create_test_state().await;

        // Create task and activity
        let task = Project::new(
            test_catalog_id(),
            "Test task".to_string(),
            "test task".to_string(),
        );
        let task_id = task.project_id;
        ts.state.project_store.create(task).await.unwrap();

        let activity = Activity::new(
            task_id,
            "ollama".to_string(),
            "qwen3-vl:8b".to_string(),
            None,
            vec![Uuid::new_v4(), Uuid::new_v4()],
        );
        let activity_id = activity.activity_id;
        ts.state.activity_store.create(activity).await.unwrap();

        let result = get_activity_results(State(ts.state.clone()), AppPath(activity_id)).await;

        assert!(result.is_ok());
        let Json(response) = result.unwrap();
        assert_eq!(response.activity_id, activity_id);
        assert!(response.results.is_empty());
        assert_eq!(response.summary.total, 2);
        assert_eq!(response.summary.completed, 0);
        assert_eq!(response.summary.failed, 0);
    }

    #[tokio::test]
    async fn test_get_activity_results_with_results() {
        let ts = create_test_state().await;

        // Create task and activity with results
        let task = Project::new(
            test_catalog_id(),
            "Test task".to_string(),
            "test task".to_string(),
        );
        let task_id = task.project_id;
        ts.state.project_store.create(task).await.unwrap();

        let photo1 = Uuid::new_v4();
        let photo2 = Uuid::new_v4();
        let mut activity = Activity::new(
            task_id,
            "ollama".to_string(),
            "qwen3-vl:8b".to_string(),
            None,
            vec![photo1, photo2],
        );

        // Add results
        activity.results.insert(
            photo1,
            crate::models::PhotoResult {
                photo_id: photo1,
                client_id: None,
                status: crate::models::PhotoResultStatus::Completed,
                tags: Some("test tags".to_string()),
                error: None,
                processed_at: Some(chrono::Utc::now()),
            },
        );

        activity.results.insert(
            photo2,
            crate::models::PhotoResult {
                photo_id: photo2,
                client_id: None,
                status: crate::models::PhotoResultStatus::Failed,
                tags: None,
                error: Some("test error".to_string()),
                processed_at: Some(chrono::Utc::now()),
            },
        );

        let activity_id = activity.activity_id;
        ts.state.activity_store.create(activity).await.unwrap();

        let result = get_activity_results(State(ts.state.clone()), AppPath(activity_id)).await;

        assert!(result.is_ok());
        let Json(response) = result.unwrap();
        assert_eq!(response.activity_id, activity_id);
        assert_eq!(response.results.len(), 2);
        assert_eq!(response.summary.total, 2);
        assert_eq!(response.summary.completed, 1);
        assert_eq!(response.summary.failed, 1);
    }

    #[tokio::test]
    async fn test_get_activity_results_not_found() {
        let ts = create_test_state().await;
        let nonexistent_activity_id = Uuid::new_v4();

        let result =
            get_activity_results(State(ts.state.clone()), AppPath(nonexistent_activity_id)).await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.body.error, "not_found");
    }

    // ========================================================================
    // Tests for retry_activity handler
    // ========================================================================

    #[tokio::test]
    async fn test_retry_activity_success() {
        let ts = create_test_state().await;

        // Create task
        let task = Project::new(
            test_catalog_id(),
            "Test task".to_string(),
            "test task".to_string(),
        );
        let task_id = task.project_id;
        ts.state.project_store.create(task).await.unwrap();

        // Create activity with mixed results: photo1 completed, photo2+photo3 failed.
        // Simulate realistic post-processing state: queued_photo_ids is empty,
        // all photos are in processed_photo_ids.
        let photo1 = Uuid::new_v4();
        let photo2 = Uuid::new_v4();
        let photo3 = Uuid::new_v4();
        let mut activity = Activity::new(
            task_id,
            "ollama".to_string(),
            "qwen3-vl:8b".to_string(),
            None,
            vec![photo1, photo2, photo3],
        );
        activity.start();
        // Simulate all photos having been processed
        activity.queued_photo_ids.clear();
        activity.processed_photo_ids = vec![photo1, photo2, photo3];
        activity.complete();

        // Add mixed results
        activity.results.insert(
            photo1,
            crate::models::PhotoResult {
                photo_id: photo1,
                client_id: None,
                status: crate::models::PhotoResultStatus::Completed,
                tags: Some("test".to_string()),
                error: None,
                processed_at: Some(chrono::Utc::now()),
            },
        );

        activity.results.insert(
            photo2,
            crate::models::PhotoResult {
                photo_id: photo2,
                client_id: None,
                status: crate::models::PhotoResultStatus::Failed,
                tags: None,
                error: Some("error".to_string()),
                processed_at: Some(chrono::Utc::now()),
            },
        );

        activity.results.insert(
            photo3,
            crate::models::PhotoResult {
                photo_id: photo3,
                client_id: None,
                status: crate::models::PhotoResultStatus::Failed,
                tags: None,
                error: Some("error".to_string()),
                processed_at: Some(chrono::Utc::now()),
            },
        );

        let original_activity_id = activity.activity_id;
        ts.state.activity_store.create(activity).await.unwrap();

        let result = retry_activity(State(ts.state.clone()), AppPath(original_activity_id)).await;

        assert!(result.is_ok());
        let Json(response) = result.unwrap();
        assert_ne!(response.activity_id, original_activity_id); // New activity ID
        assert_eq!(response.project_id, task_id);
        assert_eq!(response.status, ActivityStatus::Queued);
        assert_eq!(response.model, "qwen3-vl:8b");
        assert_eq!(response.photo_count, 2); // Only failed photos
        assert_eq!(response.parent_activity_id, original_activity_id);

        // Verify the new activity was created in storage
        let new_activity = ts
            .state
            .activity_store
            .get(response.activity_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(new_activity.photo_ids.len(), 2);
        assert!(new_activity.photo_ids.contains(&photo2));
        assert!(new_activity.photo_ids.contains(&photo3));
    }

    #[tokio::test]
    async fn test_retry_activity_not_found() {
        let ts = create_test_state().await;
        let nonexistent_activity_id = Uuid::new_v4();

        let result =
            retry_activity(State(ts.state.clone()), AppPath(nonexistent_activity_id)).await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.body.error, "not_found");
    }

    #[tokio::test]
    async fn test_retry_activity_still_processing() {
        let ts = create_test_state().await;

        // Create task and activity that's still processing
        let task = Project::new(
            test_catalog_id(),
            "Test task".to_string(),
            "test task".to_string(),
        );
        let task_id = task.project_id;
        ts.state.project_store.create(task).await.unwrap();

        let mut activity = Activity::new(
            task_id,
            "ollama".to_string(),
            "qwen3-vl:8b".to_string(),
            None,
            vec![Uuid::new_v4()],
        );
        activity.start(); // Start but don't complete
        let activity_id = activity.activity_id;
        ts.state.activity_store.create(activity).await.unwrap();

        let result = retry_activity(State(ts.state.clone()), AppPath(activity_id)).await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.body.error, "job_in_progress");
        assert!(error.body.message.contains(&activity_id.to_string()));
    }

    #[tokio::test]
    async fn test_retry_activity_no_failures() {
        let ts = create_test_state().await;

        // Create task and completed activity with no failures
        let task = Project::new(
            test_catalog_id(),
            "Test task".to_string(),
            "test task".to_string(),
        );
        let task_id = task.project_id;
        ts.state.project_store.create(task).await.unwrap();

        let photo1 = Uuid::new_v4();
        let mut activity = Activity::new(
            task_id,
            "ollama".to_string(),
            "qwen3-vl:8b".to_string(),
            None,
            vec![photo1],
        );
        activity.start();
        // Simulate photo1 having been processed
        activity.queued_photo_ids.clear();
        activity.processed_photo_ids = vec![photo1];
        activity.complete();

        // Add only successful result
        activity.results.insert(
            photo1,
            crate::models::PhotoResult {
                photo_id: photo1,
                client_id: None,
                status: crate::models::PhotoResultStatus::Completed,
                tags: Some("test".to_string()),
                error: None,
                processed_at: Some(chrono::Utc::now()),
            },
        );

        let activity_id = activity.activity_id;
        ts.state.activity_store.create(activity).await.unwrap();

        let result = retry_activity(State(ts.state.clone()), AppPath(activity_id)).await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.body.error, "no_failed_photos");
        assert!(error.body.message.contains(&activity_id.to_string()));
    }

    #[tokio::test]
    async fn test_retry_cancelled_activity_includes_unprocessed_photos() {
        let ts = create_test_state().await;

        let task = Project::new(
            test_catalog_id(),
            "Test task".to_string(),
            "test task".to_string(),
        );
        let task_id = task.project_id;
        ts.state.project_store.create(task).await.unwrap();

        // Simulate an activity that was partially processed then cancelled:
        // photo1 was processed (failed), photo2 and photo3 were never processed
        let photo1 = Uuid::new_v4();
        let photo2 = Uuid::new_v4();
        let photo3 = Uuid::new_v4();
        let mut activity = Activity::new(
            task_id,
            "ollama".to_string(),
            "qwen3-vl:8b".to_string(),
            None,
            vec![photo1, photo2, photo3],
        );
        activity.start();

        // Mark photo1 as failed (processed), leave photo2 and photo3 in queued_photo_ids
        activity.queued_photo_ids.retain(|id| id != &photo1);
        activity.processed_photo_ids.push(photo1);
        activity.results.insert(
            photo1,
            crate::models::PhotoResult {
                photo_id: photo1,
                client_id: None,
                status: crate::models::PhotoResultStatus::Failed,
                tags: None,
                error: Some("timeout".to_string()),
                processed_at: Some(chrono::Utc::now()),
            },
        );

        activity.cancel();
        let original_activity_id = activity.activity_id;
        ts.state.activity_store.create(activity).await.unwrap();

        let result = retry_activity(State(ts.state.clone()), AppPath(original_activity_id)).await;

        assert!(result.is_ok());
        let Json(response) = result.unwrap();
        // All 3 photos: 1 failed + 2 never processed
        assert_eq!(response.photo_count, 3);
        assert_eq!(response.parent_activity_id, original_activity_id);

        let new_activity = ts
            .state
            .activity_store
            .get(response.activity_id)
            .await
            .unwrap()
            .unwrap();
        assert!(new_activity.photo_ids.contains(&photo1));
        assert!(new_activity.photo_ids.contains(&photo2));
        assert!(new_activity.photo_ids.contains(&photo3));
    }

    // ========================================================================
    // Tests for cancel_activity handler
    // ========================================================================

    #[tokio::test]
    async fn test_cancel_activity_queued() {
        let ts = create_test_state().await;

        let task = Project::new(
            test_catalog_id(),
            "Test task".to_string(),
            "test task".to_string(),
        );
        let task_id = task.project_id;
        ts.state.project_store.create(task).await.unwrap();

        let activity = Activity::new(
            task_id,
            "ollama".to_string(),
            "qwen3-vl:8b".to_string(),
            None,
            vec![Uuid::new_v4()],
        );
        let activity_id = activity.activity_id;
        ts.state.activity_store.create(activity).await.unwrap();

        let result = cancel_activity(State(ts.state.clone()), AppPath(activity_id)).await;

        assert!(result.is_ok());
        let Json(response) = result.unwrap();
        assert_eq!(response.activity_id, activity_id);
        assert_eq!(response.status, ActivityStatus::Cancelled);

        // Verify the activity is cancelled in storage
        let stored_activity = ts
            .state
            .activity_store
            .get(activity_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored_activity.status, ActivityStatus::Cancelled);
    }

    #[tokio::test]
    async fn test_cancel_activity_processing() {
        let ts = create_test_state().await;

        let task = Project::new(
            test_catalog_id(),
            "Test task".to_string(),
            "test task".to_string(),
        );
        let task_id = task.project_id;
        ts.state.project_store.create(task).await.unwrap();

        let mut activity = Activity::new(
            task_id,
            "ollama".to_string(),
            "qwen3-vl:8b".to_string(),
            None,
            vec![Uuid::new_v4()],
        );
        activity.start();
        let activity_id = activity.activity_id;
        ts.state.activity_store.create(activity).await.unwrap();

        let result = cancel_activity(State(ts.state.clone()), AppPath(activity_id)).await;

        assert!(result.is_ok());
        let Json(response) = result.unwrap();
        assert_eq!(response.status, ActivityStatus::Cancelled);
    }

    #[tokio::test]
    async fn test_cancel_activity_already_finished() {
        let ts = create_test_state().await;

        let task = Project::new(
            test_catalog_id(),
            "Test task".to_string(),
            "test task".to_string(),
        );
        let task_id = task.project_id;
        ts.state.project_store.create(task).await.unwrap();

        let mut activity = Activity::new(
            task_id,
            "ollama".to_string(),
            "qwen3-vl:8b".to_string(),
            None,
            vec![Uuid::new_v4()],
        );
        activity.start();
        activity.complete();
        let activity_id = activity.activity_id;
        ts.state.activity_store.create(activity).await.unwrap();

        let result = cancel_activity(State(ts.state.clone()), AppPath(activity_id)).await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().body.error, "job_already_finished");
    }

    #[tokio::test]
    async fn test_cancel_activity_not_found() {
        let ts = create_test_state().await;
        let nonexistent_id = Uuid::new_v4();

        let result = cancel_activity(State(ts.state.clone()), AppPath(nonexistent_id)).await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().body.error, "not_found");
    }

    // ========================================================================
    // Tests for delete_activity handler
    // ========================================================================

    #[tokio::test]
    async fn test_delete_activity_queued() {
        let ts = create_test_state().await;

        // Create task and activity
        let task = Project::new(
            test_catalog_id(),
            "Test task".to_string(),
            "test task".to_string(),
        );
        let task_id = task.project_id;
        ts.state.project_store.create(task).await.unwrap();

        let activity = Activity::new(
            task_id,
            "ollama".to_string(),
            "qwen3-vl:8b".to_string(),
            None,
            vec![Uuid::new_v4()],
        );
        let activity_id = activity.activity_id;
        ts.state.activity_store.create(activity).await.unwrap();

        let result = delete_activity(State(ts.state.clone()), AppPath(activity_id)).await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().body.error, "job_not_finished");
    }

    #[tokio::test]
    async fn test_delete_activity_processing() {
        let ts = create_test_state().await;

        let task = Project::new(
            test_catalog_id(),
            "Test task".to_string(),
            "test task".to_string(),
        );
        let task_id = task.project_id;
        ts.state.project_store.create(task).await.unwrap();

        let mut activity = Activity::new(
            task_id,
            "ollama".to_string(),
            "qwen3-vl:8b".to_string(),
            None,
            vec![Uuid::new_v4()],
        );
        activity.start();
        let activity_id = activity.activity_id;
        ts.state.activity_store.create(activity).await.unwrap();

        let result = delete_activity(State(ts.state.clone()), AppPath(activity_id)).await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().body.error, "job_not_finished");
    }

    #[tokio::test]
    async fn test_delete_activity_completed() {
        let ts = create_test_state().await;

        let task = Project::new(
            test_catalog_id(),
            "Test task".to_string(),
            "test task".to_string(),
        );
        let task_id = task.project_id;
        ts.state.project_store.create(task).await.unwrap();

        let mut activity = Activity::new(
            task_id,
            "ollama".to_string(),
            "qwen3-vl:8b".to_string(),
            None,
            vec![Uuid::new_v4()],
        );
        activity.start();
        activity.complete();
        let activity_id = activity.activity_id;
        ts.state.activity_store.create(activity).await.unwrap();

        let result = delete_activity(State(ts.state.clone()), AppPath(activity_id)).await;

        assert_eq!(result, Ok(axum::http::StatusCode::NO_CONTENT));

        let deleted_activity = ts.state.activity_store.get(activity_id).await.unwrap();
        assert!(deleted_activity.is_none());
    }

    #[tokio::test]
    async fn test_delete_activity_cancelled() {
        let ts = create_test_state().await;

        let task = Project::new(
            test_catalog_id(),
            "Test task".to_string(),
            "test task".to_string(),
        );
        let task_id = task.project_id;
        ts.state.project_store.create(task).await.unwrap();

        let mut activity = Activity::new(
            task_id,
            "ollama".to_string(),
            "qwen3-vl:8b".to_string(),
            None,
            vec![Uuid::new_v4()],
        );
        activity.cancel();
        let activity_id = activity.activity_id;
        ts.state.activity_store.create(activity).await.unwrap();

        let result = delete_activity(State(ts.state.clone()), AppPath(activity_id)).await;

        assert_eq!(result, Ok(axum::http::StatusCode::NO_CONTENT));

        let deleted_activity = ts.state.activity_store.get(activity_id).await.unwrap();
        assert!(deleted_activity.is_none());
    }

    #[tokio::test]
    async fn test_delete_activity_not_found() {
        let ts = create_test_state().await;
        let nonexistent_activity_id = Uuid::new_v4();

        let result =
            delete_activity(State(ts.state.clone()), AppPath(nonexistent_activity_id)).await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.body.error, "not_found");
        assert!(
            error
                .body
                .message
                .contains(&nonexistent_activity_id.to_string())
        );
    }

    // ========================================================================
    // Tests for list_project_activities handler
    // ========================================================================

    #[tokio::test]
    async fn test_list_project_activities_empty() {
        let ts = create_test_state().await;

        let task = Project::new(
            test_catalog_id(),
            "Test task".to_string(),
            "test task".to_string(),
        );
        let task_id = task.project_id;
        ts.state.project_store.create(task).await.unwrap();

        let result = list_project_activities(State(ts.state.clone()), AppPath(task_id)).await;

        assert!(result.is_ok());
        let Json(activities) = result.unwrap();
        assert!(activities.is_empty());
    }

    #[tokio::test]
    async fn test_list_project_activities_returns_only_task_activities() {
        let ts = create_test_state().await;

        let task1 = Project::new(
            test_catalog_id(),
            "Task 1".to_string(),
            "task 1".to_string(),
        );
        let task2 = Project::new(
            test_catalog_id(),
            "Task 2".to_string(),
            "task 2".to_string(),
        );
        let task1_id = task1.project_id;
        let task2_id = task2.project_id;
        ts.state.project_store.create(task1).await.unwrap();
        ts.state.project_store.create(task2).await.unwrap();

        let activity1 = Activity::new(
            task1_id,
            "ollama".to_string(),
            "llava".to_string(),
            None,
            vec![],
        );
        let activity2 = Activity::new(
            task1_id,
            "ollama".to_string(),
            "llava".to_string(),
            None,
            vec![],
        );
        let activity3 = Activity::new(
            task2_id,
            "ollama".to_string(),
            "llava".to_string(),
            None,
            vec![],
        );
        let activity1_id = activity1.activity_id;
        let activity2_id = activity2.activity_id;
        ts.state.activity_store.create(activity1).await.unwrap();
        ts.state.activity_store.create(activity2).await.unwrap();
        ts.state.activity_store.create(activity3).await.unwrap();

        let Json(activities) = list_project_activities(State(ts.state.clone()), AppPath(task1_id))
            .await
            .unwrap();

        assert_eq!(activities.len(), 2);
        let activity_ids: Vec<Uuid> = activities.iter().map(|a| a.activity_id).collect();
        assert!(activity_ids.contains(&activity1_id));
        assert!(activity_ids.contains(&activity2_id));
    }

    #[tokio::test]
    async fn test_list_project_activities_task_not_found() {
        let ts = create_test_state().await;
        let nonexistent_task_id = Uuid::new_v4();

        let result =
            list_project_activities(State(ts.state.clone()), AppPath(nonexistent_task_id)).await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().body.error, "not_found");
    }

    #[tokio::test]
    async fn test_list_project_activities_includes_started_at() {
        let ts = create_test_state().await;

        let task = Project::new(test_catalog_id(), "Task".to_string(), "ctx".to_string());
        let task_id = task.project_id;
        ts.state.project_store.create(task).await.unwrap();

        let queued_activity = Activity::new(
            task_id,
            "ollama".to_string(),
            "llava".to_string(),
            None,
            vec![],
        );
        let mut processing_activity = Activity::new(
            task_id,
            "ollama".to_string(),
            "llava".to_string(),
            None,
            vec![],
        );
        processing_activity.start();

        ts.state
            .activity_store
            .create(queued_activity)
            .await
            .unwrap();
        ts.state
            .activity_store
            .create(processing_activity)
            .await
            .unwrap();

        let Json(activities) = list_project_activities(State(ts.state.clone()), AppPath(task_id))
            .await
            .unwrap();

        assert_eq!(activities.len(), 2);

        let queued = activities
            .iter()
            .find(|a| a.status == ActivityStatus::Queued)
            .unwrap();
        let processing = activities
            .iter()
            .find(|a| a.status == ActivityStatus::Processing)
            .unwrap();

        assert!(queued.started_at.is_none());
        assert!(processing.started_at.is_some());
    }

    // ========================================================================
    // Tests for language support
    // ========================================================================

    #[tokio::test]
    async fn test_create_activity_with_explicit_language() {
        let ts = create_test_state().await;

        let task = Project::new(test_catalog_id(), "Test".to_string(), "ctx".to_string());
        let task_id = task.project_id;
        ts.state.project_store.create(task).await.unwrap();

        let photo = Photo::new(task_id, None, "p.jpg".to_string(), 100);
        ts.state.photo_store.create(photo).await.unwrap();

        let request = CreateActivityRequest {
            provider: "test".to_string(),
            model: "qwen3-vl:8b".to_string(),
            language: Some("Italian".to_string()),
            photo_ids: None,
        };

        let result =
            create_activity(State(ts.state.clone()), AppPath(task_id), Json(request)).await;

        let (_, Json(response)) = result.unwrap();
        assert_eq!(response.language.as_deref(), Some("Italian"));

        let stored = ts
            .state
            .activity_store
            .get(response.activity_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored.language.as_deref(), Some("Italian"));
    }

    #[tokio::test]
    async fn test_create_activity_without_language_uses_config_default() {
        let mut ts = create_test_state().await;
        ts.state.config.ai.default_language = Some("French".to_string());

        let task = Project::new(test_catalog_id(), "Test".to_string(), "ctx".to_string());
        let task_id = task.project_id;
        ts.state.project_store.create(task).await.unwrap();

        let photo = Photo::new(task_id, None, "p.jpg".to_string(), 100);
        ts.state.photo_store.create(photo).await.unwrap();

        let request = CreateActivityRequest {
            provider: "test".to_string(),
            model: "qwen3-vl:8b".to_string(),
            language: None,
            photo_ids: None,
        };

        let result =
            create_activity(State(ts.state.clone()), AppPath(task_id), Json(request)).await;

        let (_, Json(response)) = result.unwrap();
        assert_eq!(
            response.language.as_deref(),
            Some("French"),
            "Should fall back to config default_language"
        );
    }

    #[tokio::test]
    async fn test_create_activity_without_language_no_config_default() {
        let ts = create_test_state().await;

        let task = Project::new(test_catalog_id(), "Test".to_string(), "ctx".to_string());
        let task_id = task.project_id;
        ts.state.project_store.create(task).await.unwrap();

        let photo = Photo::new(task_id, None, "p.jpg".to_string(), 100);
        ts.state.photo_store.create(photo).await.unwrap();

        let request = CreateActivityRequest {
            provider: "test".to_string(),
            model: "qwen3-vl:8b".to_string(),
            language: None,
            photo_ids: None,
        };

        let result =
            create_activity(State(ts.state.clone()), AppPath(task_id), Json(request)).await;

        let (_, Json(response)) = result.unwrap();
        assert!(
            response.language.is_none(),
            "Should be None when neither request nor config specifies a language"
        );
    }

    #[tokio::test]
    async fn test_create_activity_explicit_language_overrides_config_default() {
        let mut ts = create_test_state().await;
        ts.state.config.ai.default_language = Some("French".to_string());

        let task = Project::new(test_catalog_id(), "Test".to_string(), "ctx".to_string());
        let task_id = task.project_id;
        ts.state.project_store.create(task).await.unwrap();

        let photo = Photo::new(task_id, None, "p.jpg".to_string(), 100);
        ts.state.photo_store.create(photo).await.unwrap();

        let request = CreateActivityRequest {
            provider: "test".to_string(),
            model: "qwen3-vl:8b".to_string(),
            language: Some("German".to_string()),
            photo_ids: None,
        };

        let result =
            create_activity(State(ts.state.clone()), AppPath(task_id), Json(request)).await;

        let (_, Json(response)) = result.unwrap();
        assert_eq!(
            response.language.as_deref(),
            Some("German"),
            "Explicit language should override config default"
        );
    }

    #[tokio::test]
    async fn test_retry_activity_preserves_language() {
        let ts = create_test_state().await;

        let task = Project::new(test_catalog_id(), "Test".to_string(), "ctx".to_string());
        let task_id = task.project_id;
        ts.state.project_store.create(task).await.unwrap();

        let photo = Photo::new(task_id, None, "p.jpg".to_string(), 100);
        let photo_id = photo.photo_id;
        ts.state.photo_store.create(photo).await.unwrap();

        let mut activity = Activity::new(
            task_id,
            "ollama".to_string(),
            "qwen3-vl:8b".to_string(),
            Some("Italian".to_string()),
            vec![photo_id],
        );
        activity.fail();
        ts.state
            .activity_store
            .create(activity.clone())
            .await
            .unwrap();
        ts.state
            .activity_store
            .update(activity.clone())
            .await
            .unwrap();

        let result = retry_activity(State(ts.state.clone()), AppPath(activity.activity_id)).await;
        let Json(retry_response) = result.unwrap();

        assert_eq!(
            retry_response.language.as_deref(),
            Some("Italian"),
            "Retry activity should preserve language from original activity"
        );
    }

    // ========================================================================
    // Tests for provider field population
    // ========================================================================

    #[tokio::test]
    async fn test_create_activity_populates_provider() {
        let ts = create_test_state().await;

        let task = Project::new(test_catalog_id(), "Test".to_string(), "ctx".to_string());
        let task_id = task.project_id;
        ts.state.project_store.create(task).await.unwrap();

        let photo = Photo::new(task_id, None, "p.jpg".to_string(), 100);
        ts.state.photo_store.create(photo).await.unwrap();

        let request = CreateActivityRequest {
            provider: "test".to_string(),
            model: "qwen3-vl:8b".to_string(),
            language: None,
            photo_ids: None,
        };

        let (_, Json(response)) =
            create_activity(State(ts.state.clone()), AppPath(task_id), Json(request))
                .await
                .unwrap();

        assert_eq!(
            response.provider, "test",
            "ActivityResponse should record the explicitly requested provider"
        );

        let stored = ts
            .state
            .activity_store
            .get(response.activity_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            stored.provider, "test",
            "Persisted activity should record the explicitly requested provider"
        );
    }

    #[tokio::test]
    async fn test_retry_activity_preserves_provider() {
        let ts = create_test_state().await;

        let task = Project::new(test_catalog_id(), "Test".to_string(), "ctx".to_string());
        let task_id = task.project_id;
        ts.state.project_store.create(task).await.unwrap();

        let photo = Photo::new(task_id, None, "p.jpg".to_string(), 100);
        let photo_id = photo.photo_id;
        ts.state.photo_store.create(photo).await.unwrap();

        let mut activity = Activity::new(
            task_id,
            "custom-provider".to_string(),
            "qwen3-vl:8b".to_string(),
            None,
            vec![photo_id],
        );
        activity.fail();
        ts.state
            .activity_store
            .create(activity.clone())
            .await
            .unwrap();
        ts.state
            .activity_store
            .update(activity.clone())
            .await
            .unwrap();

        let Json(retry_response) =
            retry_activity(State(ts.state.clone()), AppPath(activity.activity_id))
                .await
                .unwrap();

        assert_eq!(
            retry_response.provider, "custom-provider",
            "Retry activity should preserve provider from the original activity"
        );

        let stored = ts
            .state
            .activity_store
            .get(retry_response.activity_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            stored.provider, "custom-provider",
            "Persisted retry activity should preserve provider from the original activity"
        );
    }
}
