use crate::app_state::AppState;
use crate::handlers::app_error::{AppError, AppPath};
use crate::handlers::tasks::get_existing_task;
use crate::models::{
    CreateJobRequest, Job, JobCancelledResponse, JobDetailResponse, JobResponse,
    JobResultsResponse, JobStatus, JobSummary, Photo, RetryJobResponse,
};
use crate::storage::JobStore;
use axum::Json;
use axum::extract::State;
use std::collections::HashSet;
use std::sync::Arc;
use uuid::Uuid;

/// Handler for POST /api/tasks/{task_id}/jobs
///
/// Creates a new job for processing photos with AI analysis.
pub async fn create_job(
    State(state): State<AppState>,
    AppPath(task_id): AppPath<Uuid>,
    Json(request): Json<CreateJobRequest>,
) -> Result<Json<JobResponse>, AppError> {
    get_existing_task(&state.task_store, task_id).await?;

    if !state.ai_providers.is_model_configured(&request.model) {
        return Err(AppError::bad_request(
            "invalid_model",
            format!("Model '{}' is not configured", request.model),
        ));
    }

    match state.photo_store.list_by_task(task_id).await {
        Ok(photos) => {
            let photo_ids = job_photo_ids(task_id, photos, request.photo_ids)?;
            let job = Job::new(task_id, request.model, photo_ids);
            let job_response = add_job_to_store(&state.job_store, job).await?;
            Ok(Json(job_response))
        }
        Err(e) => Err(AppError::internal_error(e.to_string())),
    }
}

fn job_photo_ids(
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

async fn add_job_to_store(
    job_store: &Arc<dyn JobStore>,
    job: Job,
) -> Result<JobResponse, AppError> {
    match job_store.create(job.clone()).await {
        Ok(job) => Ok(job.into()),
        Err(e) => Err(AppError::internal_error(e.to_string())),
    }
}

/// Helper function to retrieve a job and handle errors.
///
/// Returns the job if found, or an appropriate AppError otherwise.
async fn get_existing_job(job_store: &Arc<dyn JobStore>, job_id: Uuid) -> Result<Job, AppError> {
    match job_store.get(job_id).await {
        Ok(Some(job)) => Ok(job),
        Ok(None) => Err(AppError::job_not_found(job_id)),
        Err(e) => Err(AppError::internal_error(e.to_string())),
    }
}

/// Handler for GET /api/jobs
///
/// Lists all jobs with summary information.
pub async fn list_jobs(State(state): State<AppState>) -> Result<Json<Vec<JobSummary>>, AppError> {
    match state.job_store.list().await {
        Ok(jobs) => {
            let summaries: Vec<JobSummary> = jobs.iter().map(|job| job.into()).collect();
            Ok(Json(summaries))
        }
        Err(e) => Err(AppError::internal_error(e.to_string())),
    }
}

/// Handler for GET /api/jobs/{job_id}
///
/// Retrieves detailed information about a specific job, including progress if processing.
pub async fn get_job(
    State(state): State<AppState>,
    AppPath(job_id): AppPath<Uuid>,
) -> Result<Json<JobDetailResponse>, AppError> {
    let job = get_existing_job(&state.job_store, job_id).await?;
    let response: JobDetailResponse = job.into();
    Ok(Json(response))
}

/// Handler for GET /api/jobs/{job_id}/results
///
/// Retrieves AI analysis results for all processed photos in a job.
pub async fn get_job_results(
    State(state): State<AppState>,
    AppPath(job_id): AppPath<Uuid>,
) -> Result<Json<JobResultsResponse>, AppError> {
    let job = get_existing_job(&state.job_store, job_id).await?;
    let response: JobResultsResponse = job.into();
    Ok(Json(response))
}

/// Handler for POST /api/jobs/{job_id}/retry
///
/// Creates a new job to retry failed photos from an existing job.
pub async fn retry_job(
    State(state): State<AppState>,
    AppPath(job_id): AppPath<Uuid>,
) -> Result<Json<RetryJobResponse>, AppError> {
    let original_job = get_existing_job(&state.job_store, job_id).await?;

    // Validate job is finished
    if !original_job.is_finished() {
        return Err(AppError::conflict(
            "job_in_progress",
            format!("Cannot retry job '{}' while it is still processing", job_id),
        ));
    }

    // Get failed photo IDs
    let failed_photo_ids = original_job.failed_photo_ids();
    if failed_photo_ids.is_empty() {
        return Err(AppError::bad_request(
            "no_failed_photos",
            format!("Job '{}' has no failed photos to retry", job_id),
        ));
    }

    // Create new job with failed photos
    let new_job = Job::new(
        original_job.task_id,
        original_job.model.clone(),
        failed_photo_ids,
    );

    // Store the new job
    match state.job_store.create(new_job.clone()).await {
        Ok(created_job) => Ok(Json(RetryJobResponse {
            job_id: created_job.job_id,
            task_id: created_job.task_id,
            status: created_job.status,
            model: created_job.model,
            photo_count: created_job.photo_ids.len(),
            created_at: created_job.created_at,
            parent_job_id: job_id,
        })),
        Err(e) => Err(AppError::internal_error(e.to_string())),
    }
}

/// Handler for DELETE /api/jobs/{job_id}
///
/// Cancels and deletes a job.
pub async fn delete_job(
    State(state): State<AppState>,
    AppPath(job_id): AppPath<Uuid>,
) -> Result<Json<JobCancelledResponse>, AppError> {
    let mut job = get_existing_job(&state.job_store, job_id).await?;

    // If still processing, cancel it first
    if job.status == JobStatus::Processing {
        job.cancel();
        // Update the job in store with cancelled status
        match state.job_store.update(job.clone()).await {
            Ok(_) => {}
            Err(e) => return Err(AppError::internal_error(e.to_string())),
        }
    }

    // Delete the job from storage
    match state.job_store.delete(job_id).await {
        Ok(_) => Ok(Json(JobCancelledResponse {
            job_id,
            status: JobStatus::Cancelled,
        })),
        Err(e) => Err(AppError::internal_error(e.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::test_utils::fixtures::create_test_state;
    use crate::models::{JobStatus, Photo, Task};
    use uuid::Uuid;

    // ========================================================================
    // Tests for create_job handler
    // ========================================================================

    #[tokio::test]
    async fn test_create_job_all_photos() {
        let ts = create_test_state().await;

        // Create task with photos
        let task = Task::new("test task".to_string());
        let task_id = task.task_id;
        ts.state.task_store.create(task).await.unwrap();

        let photo1 = Photo::new(task_id, "photo1.jpg".to_string(), 1000);
        let photo2 = Photo::new(task_id, "photo2.jpg".to_string(), 2000);
        let photo1_id = photo1.photo_id;
        let photo2_id = photo2.photo_id;

        ts.state.photo_store.create(photo1).await.unwrap();
        ts.state.photo_store.create(photo2).await.unwrap();

        // Create job with all photos (photo_ids = None)
        let request = CreateJobRequest {
            model: "qwen2-vl:8b".to_string(),
            photo_ids: None,
        };

        let result = create_job(State(ts.state.clone()), AppPath(task_id), Json(request)).await;

        assert!(result.is_ok());
        let Json(job_response) = result.unwrap();
        assert_eq!(job_response.task_id, task_id);
        assert_eq!(job_response.model, "qwen2-vl:8b");
        assert_eq!(job_response.photo_count, 2);
        assert_eq!(job_response.status, JobStatus::Queued);

        // Verify the job was stored and contains all photos
        let stored_job = ts
            .state
            .job_store
            .get(job_response.job_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored_job.photo_ids.len(), 2);
        assert!(stored_job.photo_ids.contains(&photo1_id));
        assert!(stored_job.photo_ids.contains(&photo2_id));
    }

    #[tokio::test]
    async fn test_create_job_specific_photos() {
        let ts = create_test_state().await;

        // Create task with photos
        let task = Task::new("test task".to_string());
        let task_id = task.task_id;
        ts.state.task_store.create(task).await.unwrap();

        let photo1 = Photo::new(task_id, "photo1.jpg".to_string(), 1000);
        let photo2 = Photo::new(task_id, "photo2.jpg".to_string(), 2000);
        let photo3 = Photo::new(task_id, "photo3.jpg".to_string(), 3000);
        let photo1_id = photo1.photo_id;
        let photo2_id = photo2.photo_id;

        ts.state.photo_store.create(photo1).await.unwrap();
        ts.state.photo_store.create(photo2).await.unwrap();
        ts.state.photo_store.create(photo3).await.unwrap();

        // Create job with only photo1 and photo2
        let request = CreateJobRequest {
            model: "llava".to_string(),
            photo_ids: Some(vec![photo1_id, photo2_id]),
        };

        let result = create_job(State(ts.state.clone()), AppPath(task_id), Json(request)).await;

        assert!(result.is_ok());
        let Json(job_response) = result.unwrap();
        assert_eq!(job_response.task_id, task_id);
        assert_eq!(job_response.model, "llava");
        assert_eq!(job_response.photo_count, 2);

        // Verify the job contains only the specified photos
        let stored_job = ts
            .state
            .job_store
            .get(job_response.job_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored_job.photo_ids.len(), 2);
        assert!(stored_job.photo_ids.contains(&photo1_id));
        assert!(stored_job.photo_ids.contains(&photo2_id));
    }

    #[tokio::test]
    async fn test_create_job_invalid_model() {
        let ts = create_test_state().await;

        // Create task (test state uses an empty ProviderRegistry — no models configured)
        let task = Task::new("test task".to_string());
        let task_id = task.task_id;
        ts.state.task_store.create(task).await.unwrap();

        let request = CreateJobRequest {
            model: "nonexistent-model".to_string(),
            photo_ids: None,
        };

        let result = create_job(State(ts.state.clone()), AppPath(task_id), Json(request)).await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.body.error, "invalid_model");
        assert!(error.body.message.contains("nonexistent-model"));
    }

    #[tokio::test]
    async fn test_create_job_task_not_found() {
        let ts = create_test_state().await;
        let nonexistent_task_id = Uuid::new_v4();

        let request = CreateJobRequest {
            model: "qwen2-vl:8b".to_string(),
            photo_ids: None,
        };

        let result = create_job(
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
    async fn test_create_job_invalid_photo_id() {
        let ts = create_test_state().await;

        // Create task with one photo
        let task = Task::new("test task".to_string());
        let task_id = task.task_id;
        ts.state.task_store.create(task).await.unwrap();

        let photo1 = Photo::new(task_id, "photo1.jpg".to_string(), 1000);
        ts.state.photo_store.create(photo1).await.unwrap();

        // Try to create job with a photo_id that doesn't belong to this task
        let invalid_photo_id = Uuid::new_v4();
        let request = CreateJobRequest {
            model: "qwen2-vl:8b".to_string(),
            photo_ids: Some(vec![invalid_photo_id]),
        };

        let result = create_job(State(ts.state.clone()), AppPath(task_id), Json(request)).await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.body.error, "invalid_parameter");
        assert!(error.body.message.contains(&invalid_photo_id.to_string()));
        assert!(error.body.message.contains(&task_id.to_string()));
    }

    #[tokio::test]
    async fn test_create_job_empty_task() {
        let ts = create_test_state().await;

        // Create task without photos
        let task = Task::new("empty task".to_string());
        let task_id = task.task_id;
        ts.state.task_store.create(task).await.unwrap();

        // Create job for empty task
        let request = CreateJobRequest {
            model: "qwen2-vl:8b".to_string(),
            photo_ids: None,
        };

        let result = create_job(State(ts.state.clone()), AppPath(task_id), Json(request)).await;

        // Should succeed but with 0 photos
        assert!(result.is_ok());
        let Json(job_response) = result.unwrap();
        assert_eq!(job_response.task_id, task_id);
        assert_eq!(job_response.photo_count, 0);

        // Verify the job has no photos
        let stored_job = ts
            .state
            .job_store
            .get(job_response.job_id)
            .await
            .unwrap()
            .unwrap();
        assert!(stored_job.photo_ids.is_empty());
    }

    // ========================================================================
    // Tests for list_jobs handler
    // ========================================================================

    #[tokio::test]
    async fn test_list_jobs_empty() {
        let ts = create_test_state().await;

        let result = list_jobs(State(ts.state.clone())).await;

        assert!(result.is_ok());
        let Json(jobs) = result.unwrap();
        assert!(jobs.is_empty());
    }

    #[tokio::test]
    async fn test_list_jobs_multiple() {
        let ts = create_test_state().await;

        // Create task with photos
        let task = Task::new("test task".to_string());
        let task_id = task.task_id;
        ts.state.task_store.create(task).await.unwrap();

        let photo1 = Photo::new(task_id, "photo1.jpg".to_string(), 1000);
        ts.state.photo_store.create(photo1).await.unwrap();

        // Create multiple jobs
        let job1 = Job::new(task_id, "qwen2-vl:8b".to_string(), vec![]);
        let job2 = Job::new(task_id, "llava".to_string(), vec![]);
        ts.state.job_store.create(job1.clone()).await.unwrap();
        ts.state.job_store.create(job2.clone()).await.unwrap();

        let result = list_jobs(State(ts.state.clone())).await;

        assert!(result.is_ok());
        let Json(jobs) = result.unwrap();
        assert_eq!(jobs.len(), 2);
        assert_eq!(jobs[0].status, JobStatus::Queued);
        assert_eq!(jobs[1].status, JobStatus::Queued);
    }

    #[tokio::test]
    async fn test_list_jobs_mixed_statuses() {
        let ts = create_test_state().await;

        // Create task
        let task = Task::new("test task".to_string());
        let task_id = task.task_id;
        ts.state.task_store.create(task).await.unwrap();

        // Create jobs with different statuses
        let mut job1 = Job::new(task_id, "qwen2-vl:8b".to_string(), vec![]);
        job1.start();
        let mut job2 = Job::new(task_id, "llava".to_string(), vec![]);
        job2.start();
        job2.complete();

        ts.state.job_store.create(job1.clone()).await.unwrap();
        ts.state.job_store.create(job2.clone()).await.unwrap();

        let result = list_jobs(State(ts.state.clone())).await;

        assert!(result.is_ok());
        let Json(jobs) = result.unwrap();
        assert_eq!(jobs.len(), 2);
        // Check that we have both processing and completed
        let statuses: Vec<JobStatus> = jobs.iter().map(|j| j.status).collect();
        assert!(statuses.contains(&JobStatus::Processing));
        assert!(statuses.contains(&JobStatus::Completed));
    }

    // ========================================================================
    // Tests for get_job handler
    // ========================================================================

    #[tokio::test]
    async fn test_get_job_found() {
        let ts = create_test_state().await;

        // Create task and job
        let task = Task::new("test task".to_string());
        let task_id = task.task_id;
        ts.state.task_store.create(task).await.unwrap();

        let job = Job::new(
            task_id,
            "qwen2-vl:8b".to_string(),
            vec![Uuid::new_v4(), Uuid::new_v4()],
        );
        let job_id = job.job_id;
        ts.state.job_store.create(job).await.unwrap();

        let result = get_job(State(ts.state.clone()), AppPath(job_id)).await;

        assert!(result.is_ok());
        let Json(response) = result.unwrap();
        assert_eq!(response.job_id, job_id);
        assert_eq!(response.task_id, task_id);
        assert_eq!(response.status, JobStatus::Queued);
        assert_eq!(response.photo_count, 2);
        assert!(response.progress.is_none()); // No progress when queued
    }

    #[tokio::test]
    async fn test_get_job_with_progress() {
        let ts = create_test_state().await;

        // Create task and job
        let task = Task::new("test task".to_string());
        let task_id = task.task_id;
        ts.state.task_store.create(task).await.unwrap();

        let mut job = Job::new(
            task_id,
            "qwen2-vl:8b".to_string(),
            vec![Uuid::new_v4(), Uuid::new_v4()],
        );
        job.start();
        let job_id = job.job_id;
        ts.state.job_store.create(job).await.unwrap();

        let result = get_job(State(ts.state.clone()), AppPath(job_id)).await;

        assert!(result.is_ok());
        let Json(response) = result.unwrap();
        assert_eq!(response.status, JobStatus::Processing);
        assert!(response.progress.is_some()); // Should have progress when processing
        let progress = response.progress.unwrap();
        assert_eq!(progress.completed, 0);
        assert_eq!(progress.failed, 0);
        assert_eq!(progress.remaining, 2);
    }

    #[tokio::test]
    async fn test_get_job_not_found() {
        let ts = create_test_state().await;
        let nonexistent_job_id = Uuid::new_v4();

        let result = get_job(State(ts.state.clone()), AppPath(nonexistent_job_id)).await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.body.error, "not_found");
        assert!(error.body.message.contains(&nonexistent_job_id.to_string()));
    }

    // ========================================================================
    // Tests for get_job_results handler
    // ========================================================================

    #[tokio::test]
    async fn test_get_job_results_empty() {
        let ts = create_test_state().await;

        // Create task and job
        let task = Task::new("test task".to_string());
        let task_id = task.task_id;
        ts.state.task_store.create(task).await.unwrap();

        let job = Job::new(
            task_id,
            "qwen2-vl:8b".to_string(),
            vec![Uuid::new_v4(), Uuid::new_v4()],
        );
        let job_id = job.job_id;
        ts.state.job_store.create(job).await.unwrap();

        let result = get_job_results(State(ts.state.clone()), AppPath(job_id)).await;

        assert!(result.is_ok());
        let Json(response) = result.unwrap();
        assert_eq!(response.job_id, job_id);
        assert!(response.results.is_empty());
        assert_eq!(response.summary.total, 2);
        assert_eq!(response.summary.completed, 0);
        assert_eq!(response.summary.failed, 0);
    }

    #[tokio::test]
    async fn test_get_job_results_with_results() {
        let ts = create_test_state().await;

        // Create task and job with results
        let task = Task::new("test task".to_string());
        let task_id = task.task_id;
        ts.state.task_store.create(task).await.unwrap();

        let photo1 = Uuid::new_v4();
        let photo2 = Uuid::new_v4();
        let mut job = Job::new(task_id, "qwen2-vl:8b".to_string(), vec![photo1, photo2]);

        // Add results
        job.results.insert(
            photo1,
            crate::models::PhotoResult {
                photo_id: photo1,
                status: crate::models::PhotoResultStatus::Completed,
                tags: Some("test tags".to_string()),
                error: None,
                processed_at: Some(chrono::Utc::now()),
            },
        );

        job.results.insert(
            photo2,
            crate::models::PhotoResult {
                photo_id: photo2,
                status: crate::models::PhotoResultStatus::Failed,
                tags: None,
                error: Some("test error".to_string()),
                processed_at: Some(chrono::Utc::now()),
            },
        );

        let job_id = job.job_id;
        ts.state.job_store.create(job).await.unwrap();

        let result = get_job_results(State(ts.state.clone()), AppPath(job_id)).await;

        assert!(result.is_ok());
        let Json(response) = result.unwrap();
        assert_eq!(response.job_id, job_id);
        assert_eq!(response.results.len(), 2);
        assert_eq!(response.summary.total, 2);
        assert_eq!(response.summary.completed, 1);
        assert_eq!(response.summary.failed, 1);
    }

    #[tokio::test]
    async fn test_get_job_results_not_found() {
        let ts = create_test_state().await;
        let nonexistent_job_id = Uuid::new_v4();

        let result = get_job_results(State(ts.state.clone()), AppPath(nonexistent_job_id)).await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.body.error, "not_found");
    }

    // ========================================================================
    // Tests for retry_job handler
    // ========================================================================

    #[tokio::test]
    async fn test_retry_job_success() {
        let ts = create_test_state().await;

        // Create task
        let task = Task::new("test task".to_string());
        let task_id = task.task_id;
        ts.state.task_store.create(task).await.unwrap();

        // Create job with failed photos
        let photo1 = Uuid::new_v4();
        let photo2 = Uuid::new_v4();
        let photo3 = Uuid::new_v4();
        let mut job = Job::new(
            task_id,
            "qwen2-vl:8b".to_string(),
            vec![photo1, photo2, photo3],
        );
        job.start();
        job.complete();

        // Add mixed results
        job.results.insert(
            photo1,
            crate::models::PhotoResult {
                photo_id: photo1,
                status: crate::models::PhotoResultStatus::Completed,
                tags: Some("test".to_string()),
                error: None,
                processed_at: Some(chrono::Utc::now()),
            },
        );

        job.results.insert(
            photo2,
            crate::models::PhotoResult {
                photo_id: photo2,
                status: crate::models::PhotoResultStatus::Failed,
                tags: None,
                error: Some("error".to_string()),
                processed_at: Some(chrono::Utc::now()),
            },
        );

        job.results.insert(
            photo3,
            crate::models::PhotoResult {
                photo_id: photo3,
                status: crate::models::PhotoResultStatus::Failed,
                tags: None,
                error: Some("error".to_string()),
                processed_at: Some(chrono::Utc::now()),
            },
        );

        let original_job_id = job.job_id;
        ts.state.job_store.create(job).await.unwrap();

        let result = retry_job(State(ts.state.clone()), AppPath(original_job_id)).await;

        assert!(result.is_ok());
        let Json(response) = result.unwrap();
        assert_ne!(response.job_id, original_job_id); // New job ID
        assert_eq!(response.task_id, task_id);
        assert_eq!(response.status, JobStatus::Queued);
        assert_eq!(response.model, "qwen2-vl:8b");
        assert_eq!(response.photo_count, 2); // Only failed photos
        assert_eq!(response.parent_job_id, original_job_id);

        // Verify the new job was created in storage
        let new_job = ts
            .state
            .job_store
            .get(response.job_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(new_job.photo_ids.len(), 2);
        assert!(new_job.photo_ids.contains(&photo2));
        assert!(new_job.photo_ids.contains(&photo3));
    }

    #[tokio::test]
    async fn test_retry_job_not_found() {
        let ts = create_test_state().await;
        let nonexistent_job_id = Uuid::new_v4();

        let result = retry_job(State(ts.state.clone()), AppPath(nonexistent_job_id)).await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.body.error, "not_found");
    }

    #[tokio::test]
    async fn test_retry_job_still_processing() {
        let ts = create_test_state().await;

        // Create task and job that's still processing
        let task = Task::new("test task".to_string());
        let task_id = task.task_id;
        ts.state.task_store.create(task).await.unwrap();

        let mut job = Job::new(task_id, "qwen2-vl:8b".to_string(), vec![Uuid::new_v4()]);
        job.start(); // Start but don't complete
        let job_id = job.job_id;
        ts.state.job_store.create(job).await.unwrap();

        let result = retry_job(State(ts.state.clone()), AppPath(job_id)).await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.body.error, "job_in_progress");
        assert!(error.body.message.contains(&job_id.to_string()));
    }

    #[tokio::test]
    async fn test_retry_job_no_failures() {
        let ts = create_test_state().await;

        // Create task and completed job with no failures
        let task = Task::new("test task".to_string());
        let task_id = task.task_id;
        ts.state.task_store.create(task).await.unwrap();

        let photo1 = Uuid::new_v4();
        let mut job = Job::new(task_id, "qwen2-vl:8b".to_string(), vec![photo1]);
        job.start();
        job.complete();

        // Add only successful result
        job.results.insert(
            photo1,
            crate::models::PhotoResult {
                photo_id: photo1,
                status: crate::models::PhotoResultStatus::Completed,
                tags: Some("test".to_string()),
                error: None,
                processed_at: Some(chrono::Utc::now()),
            },
        );

        let job_id = job.job_id;
        ts.state.job_store.create(job).await.unwrap();

        let result = retry_job(State(ts.state.clone()), AppPath(job_id)).await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.body.error, "no_failed_photos");
        assert!(error.body.message.contains(&job_id.to_string()));
    }

    // ========================================================================
    // Tests for delete_job handler
    // ========================================================================

    #[tokio::test]
    async fn test_delete_job_queued() {
        let ts = create_test_state().await;

        // Create task and job
        let task = Task::new("test task".to_string());
        let task_id = task.task_id;
        ts.state.task_store.create(task).await.unwrap();

        let job = Job::new(task_id, "qwen2-vl:8b".to_string(), vec![Uuid::new_v4()]);
        let job_id = job.job_id;
        ts.state.job_store.create(job).await.unwrap();

        let result = delete_job(State(ts.state.clone()), AppPath(job_id)).await;

        assert!(result.is_ok());
        let Json(response) = result.unwrap();
        assert_eq!(response.job_id, job_id);
        assert_eq!(response.status, JobStatus::Cancelled);

        // Verify job was deleted
        let deleted_job = ts.state.job_store.get(job_id).await.unwrap();
        assert!(deleted_job.is_none());
    }

    #[tokio::test]
    async fn test_delete_job_processing() {
        let ts = create_test_state().await;

        // Create task and processing job
        let task = Task::new("test task".to_string());
        let task_id = task.task_id;
        ts.state.task_store.create(task).await.unwrap();

        let mut job = Job::new(task_id, "qwen2-vl:8b".to_string(), vec![Uuid::new_v4()]);
        job.start();
        let job_id = job.job_id;
        ts.state.job_store.create(job).await.unwrap();

        let result = delete_job(State(ts.state.clone()), AppPath(job_id)).await;

        assert!(result.is_ok());
        let Json(response) = result.unwrap();
        assert_eq!(response.job_id, job_id);
        assert_eq!(response.status, JobStatus::Cancelled);

        // Verify job was deleted
        let deleted_job = ts.state.job_store.get(job_id).await.unwrap();
        assert!(deleted_job.is_none());
    }

    #[tokio::test]
    async fn test_delete_job_completed() {
        let ts = create_test_state().await;

        // Create task and completed job
        let task = Task::new("test task".to_string());
        let task_id = task.task_id;
        ts.state.task_store.create(task).await.unwrap();

        let mut job = Job::new(task_id, "qwen2-vl:8b".to_string(), vec![Uuid::new_v4()]);
        job.start();
        job.complete();
        let job_id = job.job_id;
        ts.state.job_store.create(job).await.unwrap();

        let result = delete_job(State(ts.state.clone()), AppPath(job_id)).await;

        assert!(result.is_ok());
        let Json(response) = result.unwrap();
        assert_eq!(response.job_id, job_id);
        assert_eq!(response.status, JobStatus::Cancelled);

        // Verify job was deleted
        let deleted_job = ts.state.job_store.get(job_id).await.unwrap();
        assert!(deleted_job.is_none());
    }

    #[tokio::test]
    async fn test_delete_job_not_found() {
        let ts = create_test_state().await;
        let nonexistent_job_id = Uuid::new_v4();

        let result = delete_job(State(ts.state.clone()), AppPath(nonexistent_job_id)).await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.body.error, "not_found");
        assert!(error.body.message.contains(&nonexistent_job_id.to_string()));
    }
}
