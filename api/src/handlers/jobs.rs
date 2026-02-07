use crate::app_state::AppState;
use crate::handlers::app_error::{AppError, AppPath};
use crate::handlers::tasks::get_existing_task;
use crate::models::{CreateJobRequest, Job, JobResponse, Photo};
use axum::extract::State;
use axum::Json;
use std::collections::HashSet;
use std::sync::Arc;
use uuid::Uuid;
use crate::storage::JobStore;

pub async fn create_job(
    State(state): State<AppState>,
    AppPath(task_id): AppPath<Uuid>,
    Json(request): Json<CreateJobRequest>,
) -> Result<Json<JobResponse>, AppError> {
    get_existing_task(&state.task_store, task_id).await?;
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

fn job_photo_ids(task_id: Uuid, photos: Vec<Photo>, request_photo_ids: Option<Vec<Uuid>>) -> Result<Vec<Uuid>, AppError> {
    let photo_ids = match request_photo_ids {
        Some(photo_ids) => {
            let task_photo_ids = photos.iter().map(|p| &p.photo_id).collect::<HashSet<_>>();
            for photo_id in photo_ids.clone() {
                if !task_photo_ids.contains(&photo_id) {
                    return Err(AppError::bad_request("invalid_parameter", format!("photo with photo_id '{}' does not exists in task '{}'", photo_id, task_id)));
                }
            }
            photo_ids
        }
        None => photos.iter().map(|p| p.photo_id).collect(),
    };
    Ok(photo_ids)
}

async fn add_job_to_store(job_store: &Arc<dyn JobStore>, job: Job) -> Result<JobResponse, AppError> {
    match job_store.create(job.clone()).await {
        Ok(job) => Ok(job.into()),
        Err(e) => Err(AppError::internal_error(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_state::AppState;
    use crate::config::Config;
    use crate::models::{JobStatus, Photo, Task};
    use crate::services::ai::ProviderRegistry;
    use crate::storage::{FileSystemPhotoStore, FileSystemTaskStore, InMemoryJobStore};
    use std::sync::Arc;
    use tempfile::TempDir;
    use uuid::Uuid;

    struct TestState {
        state: AppState,
        _temp_dir: TempDir,
    }

    async fn create_test_state() -> TestState {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let storage_path = temp_dir.path().to_path_buf();
        let config = Config::default();
        let task_store = Arc::new(FileSystemTaskStore::new(storage_path.clone()).await);
        let photo_store = Arc::new(FileSystemPhotoStore::new(storage_path).await);
        let job_store = Arc::new(InMemoryJobStore::new());
        let ai_providers = Arc::new(ProviderRegistry::new());
        TestState {
            state: AppState::new(config, task_store, photo_store, job_store, ai_providers),
            _temp_dir: temp_dir,
        }
    }

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
        let stored_job = ts.state.job_store.get(job_response.job_id).await.unwrap().unwrap();
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
        let stored_job = ts.state.job_store.get(job_response.job_id).await.unwrap().unwrap();
        assert_eq!(stored_job.photo_ids.len(), 2);
        assert!(stored_job.photo_ids.contains(&photo1_id));
        assert!(stored_job.photo_ids.contains(&photo2_id));
    }

    #[tokio::test]
    async fn test_create_job_task_not_found() {
        let ts = create_test_state().await;
        let nonexistent_task_id = Uuid::new_v4();

        let request = CreateJobRequest {
            model: "qwen2-vl:8b".to_string(),
            photo_ids: None,
        };

        let result = create_job(State(ts.state.clone()), AppPath(nonexistent_task_id), Json(request)).await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.body.error, "not_found");
        assert!(error.body.message.contains(&nonexistent_task_id.to_string()));
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
        let stored_job = ts.state.job_store.get(job_response.job_id).await.unwrap().unwrap();
        assert!(stored_job.photo_ids.is_empty());
    }
}