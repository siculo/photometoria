use crate::app_state::AppState;
use crate::handlers::app_error::{AppError, AppPath};
use crate::handlers::tasks::get_existing_task;
use crate::models::{Photo, PhotoListResponse, PhotoResponse};
use crate::storage::PhotoStore;
use axum::extract::State;
use axum::Json;
use std::sync::Arc;
use axum::http::StatusCode;
use uuid::Uuid;

pub async fn task_photos(
    State(state): State<AppState>,
    AppPath(task_id): AppPath<Uuid>,
) -> Result<Json<PhotoListResponse>, AppError> {
    get_existing_task(&state.task_store, task_id).await?;
    list_task_photo(&state.photo_store, task_id).await
}

pub async fn get_photo(
    State(state): State<AppState>,
    AppPath(photo_id): AppPath<Uuid>,
) -> Result<Json<PhotoResponse>, AppError> {
    let photo = get_existing_photo(&state.photo_store, photo_id).await?;
    Ok(Json(photo.into()))
}

pub async fn delete_photo(
    State(state): State<AppState>,
    AppPath(photo_id): AppPath<Uuid>,
) -> Result<StatusCode, AppError> {
    get_existing_photo(&state.photo_store, photo_id).await?;
    match state.photo_store.delete(photo_id).await {
        Ok(_) => Ok(StatusCode::NO_CONTENT),
        Err(error) => Err(AppError::internal_error(error.to_string()))
    }
}

async fn list_task_photo(
    photo_store: &Arc<dyn PhotoStore>,
    task_id: Uuid,
) -> Result<Json<PhotoListResponse>, AppError> {
    match photo_store.list_by_task(task_id).await {
        Ok(photos) => {
            let count = photos.len();
            let mut photo_ids: Vec<Uuid> = Vec::new();
            for photo in photos {
                photo_ids.push(photo.photo_id);
            }
            Ok(Json(PhotoListResponse { photo_ids, count }))
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
        Ok(None) => Err(AppError::not_found(format!("Photo with id {} not found", photo_id))),
        Err(e) => Err(AppError::internal_error(e.to_string()))
    }
}