use crate::app_state::AppState;
use crate::handlers::app_error::{AppError, AppPath};
use crate::models::PhotoListResponse;
use crate::storage::PhotoStore;
use axum::extract::State;
use axum::Json;
use std::sync::Arc;
use uuid::Uuid;

pub async fn task_photos(
    State(state): State<AppState>,
    AppPath(task_id): AppPath<Uuid>,
) -> Result<Json<PhotoListResponse>, AppError> {
    match state.task_store.get(task_id).await {
        Ok(Some(_)) => Ok(list_task_photo(state.photo_store, task_id).await?),
        Ok(None) => Err(AppError::task_not_found(task_id)),
        Err(e) => Err(AppError::internal_error(e.to_string())),
    }
}

async fn list_task_photo(
    photo_store: Arc<dyn PhotoStore>,
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

#[cfg(test)]
mod tests {

}
