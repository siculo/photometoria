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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::test_utils::fixtures::create_test_state;
    use crate::models::{Photo, Task};
    use uuid::Uuid;

    // ========================================================================
    // Tests for task_photos handler
    // ========================================================================

    #[tokio::test]
    async fn test_task_photos_empty() {
        let ts = create_test_state().await;

        // Create a task without photos
        let task = Task::new("test task".to_string());
        let task_id = task.task_id;
        ts.state.task_store.create(task).await.unwrap();

        let result = task_photos(State(ts.state.clone()), AppPath(task_id)).await;

        assert!(result.is_ok());
        let Json(photo_list) = result.unwrap();
        assert_eq!(photo_list.photo_ids.len(), 0);
        assert_eq!(photo_list.count, 0);
    }

    #[tokio::test]
    async fn test_task_photos_with_photos() {
        let ts = create_test_state().await;

        // Create a task
        let task = Task::new("test task with photos".to_string());
        let task_id = task.task_id;
        ts.state.task_store.create(task).await.unwrap();

        // Create multiple photos
        let photo1 = Photo::new(task_id, "photo1.jpg".to_string(), 1000);
        let photo2 = Photo::new(task_id, "photo2.jpg".to_string(), 2000);
        let photo3 = Photo::new(task_id, "photo3.jpg".to_string(), 3000);

        let photo1_id = photo1.photo_id;
        let photo2_id = photo2.photo_id;
        let photo3_id = photo3.photo_id;

        ts.state.photo_store.create(photo1).await.unwrap();
        ts.state.photo_store.create(photo2).await.unwrap();
        ts.state.photo_store.create(photo3).await.unwrap();

        let result = task_photos(State(ts.state.clone()), AppPath(task_id)).await;

        assert!(result.is_ok());
        let Json(photo_list) = result.unwrap();
        assert_eq!(photo_list.count, 3);
        assert_eq!(photo_list.photo_ids.len(), 3);
        assert!(photo_list.photo_ids.contains(&photo1_id));
        assert!(photo_list.photo_ids.contains(&photo2_id));
        assert!(photo_list.photo_ids.contains(&photo3_id));
    }

    #[tokio::test]
    async fn test_task_photos_task_not_found() {
        let ts = create_test_state().await;
        let nonexistent_id = Uuid::new_v4();

        let result = task_photos(State(ts.state.clone()), AppPath(nonexistent_id)).await;

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
        let task = Task::new("test task".to_string());
        let task_id = task.task_id;
        ts.state.task_store.create(task).await.unwrap();

        let photo = Photo::new(task_id, "test.jpg".to_string(), 1_234_567);
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
        let task = Task::new("test task".to_string());
        let task_id = task.task_id;
        ts.state.task_store.create(task).await.unwrap();

        let photo = Photo::new(task_id, "to_delete.jpg".to_string(), 5000);
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
}