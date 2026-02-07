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