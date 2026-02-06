use axum::{
    Router,
    extract::DefaultBodyLimit,
    routing::{get, post},
};
use crate::app_state::AppState;
use crate::handlers::photos::{delete_photo, get_photo, task_photos};
use crate::handlers::upload_photos::upload_photos;
use crate::handlers::tasks::{create_task, delete_task, get_task, list_tasks, update_task};

pub fn create_router(state: AppState) -> Router {
    // Calculate max body size for uploads: max_photo_size * max_photos + overhead
    let max_upload_size = state.config.upload.max_photo_size.0 as usize
        * state.config.upload.max_photos_per_request
        + 1024 * 1024; // 1MB overhead for multipart boundaries

    Router::new()
        .route("/version", get(version))
        .route("/api/tasks", post(create_task).get(list_tasks))
        .route(
            "/api/tasks/{task_id}",
            get(get_task).patch(update_task).delete(delete_task),
        )
        .route(
            "/api/tasks/{task_id}/photos",
            get(task_photos).post(upload_photos).layer(DefaultBodyLimit::max(max_upload_size)),
        )
        .route(
            "/api/photos/{photo_id}",
            get(get_photo).delete(delete_photo)
        )
        .with_state(state)
}

async fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use http_body_util::BodyExt;
    use serde_json::json;
    use tempfile::TempDir;
    use tower::ServiceExt;

    use uuid::Uuid;
    use crate::app_state::AppState;
    use crate::config::Config;
    use crate::models::{TaskDetail, TaskResponse, TaskSummary};
    use crate::services::ai::ProviderRegistry;
    use crate::storage::{FileSystemPhotoStore, FileSystemTaskStore, PhotoStore, TaskStore};

    struct TestApp {
        router: Router,
        state: AppState,
        _temp_dir: TempDir,
    }

    async fn create_test_app() -> TestApp {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let storage_path = temp_dir.path().to_path_buf();
        let config = Config::default();
        let task_store: Arc<dyn TaskStore> = Arc::new(FileSystemTaskStore::new(storage_path.clone()).await);
        let photo_store: Arc<dyn PhotoStore> = Arc::new(FileSystemPhotoStore::new(storage_path).await);
        let ai_providers = Arc::new(ProviderRegistry::new());
        let state = AppState::new(config, task_store, photo_store, ai_providers);
        let router = create_router(state.clone());
        TestApp {
            router,
            state,
            _temp_dir: temp_dir,
        }
    }

    #[tokio::test]
    async fn test_version_returns_package_version() {
        let ta = create_test_app().await;
        let request = Request::get("/version").body(Body::empty()).unwrap();
        let response = ta.router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), 200);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let body_str = String::from_utf8(body.to_vec()).unwrap();

        assert_eq!(body_str, env!("CARGO_PKG_VERSION"));
    }

    #[tokio::test]
    async fn test_post_tasks_creates_task() {
        let ta = create_test_app().await;
        let request = Request::post("/api/tasks")
            .header("Content-Type", "application/json")
            .body(Body::from(json!({"context": "test context"}).to_string()))
            .unwrap();

        let response = ta.router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::CREATED);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let task_response: TaskResponse = serde_json::from_slice(&body).unwrap();

        assert_eq!(task_response.context, "test context");
        assert!(!task_response.task_id.is_nil());
    }

    #[tokio::test]
    async fn test_get_tasks_returns_empty_list() {
        let ta = create_test_app().await;
        let request = Request::get("/api/tasks").body(Body::empty()).unwrap();

        let response = ta.router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let summaries: Vec<TaskSummary> = serde_json::from_slice(&body).unwrap();

        assert!(summaries.is_empty());
    }

    #[tokio::test]
    async fn test_get_tasks_returns_created_tasks() {
        let ta = create_test_app().await;

        // Create a task first using the store directly
        let task = crate::models::Task::new("test task".to_string());
        let task_id = task.task_id.clone();
        ta.state.task_store.create(task).await.unwrap();

        let request = Request::get("/api/tasks").body(Body::empty()).unwrap();
        let response = ta.router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let summaries: Vec<TaskSummary> = serde_json::from_slice(&body).unwrap();

        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].task_id, task_id);
        assert_eq!(summaries[0].context, "test task");
    }

    #[tokio::test]
    async fn test_get_task_by_id_found() {
        let ta = create_test_app().await;

        // Create a task first
        let task = crate::models::Task::new("test task".to_string());
        let task_id = task.task_id.clone();
        ta.state.task_store.create(task).await.unwrap();

        let request = Request::get(format!("/api/tasks/{}", task_id))
            .body(Body::empty())
            .unwrap();
        let response = ta.router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let detail: TaskDetail = serde_json::from_slice(&body).unwrap();

        assert_eq!(detail.task_id, task_id);
        assert_eq!(detail.context, "test task");
    }

    #[tokio::test]
    async fn test_get_task_by_id_not_found() {
        let ta = create_test_app().await;
        let nonexistent_id = Uuid::new_v4();

        let request = Request::get(format!("/api/tasks/{}", nonexistent_id))
            .body(Body::empty())
            .unwrap();
        let response = ta.router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_patch_task_updates_context() {
        let ta = create_test_app().await;

        // Create a task first
        let task = crate::models::Task::new("original context".to_string());
        let task_id = task.task_id.clone();
        ta.state.task_store.create(task).await.unwrap();

        let request = Request::patch(format!("/api/tasks/{}", task_id))
            .header("Content-Type", "application/json")
            .body(Body::from(
                json!({"context": "updated context"}).to_string(),
            ))
            .unwrap();
        let response = ta.router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let task_response: TaskResponse = serde_json::from_slice(&body).unwrap();

        assert_eq!(task_response.task_id, task_id);
        assert_eq!(task_response.context, "updated context");
    }

    #[tokio::test]
    async fn test_patch_task_not_found() {
        let ta = create_test_app().await;
        let nonexistent_id = Uuid::new_v4();

        let request = Request::patch(format!("/api/tasks/{}", nonexistent_id))
            .header("Content-Type", "application/json")
            .body(Body::from(
                json!({"context": "updated context"}).to_string(),
            ))
            .unwrap();
        let response = ta.router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_delete_task_success() {
        let ta = create_test_app().await;

        // Create a task first
        let task = crate::models::Task::new("task to delete".to_string());
        let task_id = task.task_id.clone();
        ta.state.task_store.create(task).await.unwrap();

        let request = Request::delete(format!("/api/tasks/{}", task_id))
            .body(Body::empty())
            .unwrap();
        let response = ta.router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        // Verify it's deleted
        let exists = ta.state.task_store.exists(task_id).await.unwrap();
        assert!(!exists);
    }

    #[tokio::test]
    async fn test_delete_task_not_found() {
        let ta = create_test_app().await;
        let nonexistent_id = Uuid::new_v4();

        let request = Request::delete(format!("/api/tasks/{}", nonexistent_id))
            .body(Body::empty())
            .unwrap();
        let response = ta.router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_list_task_photos_empty() {
        let ta = create_test_app().await;

        // Create a task without photos
        let task = crate::models::Task::new("test task".to_string());
        let task_id = task.task_id;
        ta.state.task_store.create(task).await.unwrap();

        let request = Request::get(format!("/api/tasks/{}/photos", task_id))
            .body(Body::empty())
            .unwrap();
        let response = ta.router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let photo_list: crate::models::PhotoListResponse = serde_json::from_slice(&body).unwrap();

        assert_eq!(photo_list.photo_ids.len(), 0);
        assert_eq!(photo_list.count, 0);
    }

    #[tokio::test]
    async fn test_list_task_photos_with_photos() {
        let ta = create_test_app().await;

        // Create a task
        let task = crate::models::Task::new("test task with photos".to_string());
        let task_id = task.task_id;
        ta.state.task_store.create(task).await.unwrap();

        // Create multiple photos
        let photo1 = crate::models::Photo::new(task_id, "photo1.jpg".to_string(), 1000);
        let photo2 = crate::models::Photo::new(task_id, "photo2.jpg".to_string(), 2000);
        let photo3 = crate::models::Photo::new(task_id, "photo3.jpg".to_string(), 3000);

        let photo1_id = photo1.photo_id;
        let photo2_id = photo2.photo_id;
        let photo3_id = photo3.photo_id;

        ta.state.photo_store.create(photo1).await.unwrap();
        ta.state.photo_store.create(photo2).await.unwrap();
        ta.state.photo_store.create(photo3).await.unwrap();

        let request = Request::get(format!("/api/tasks/{}/photos", task_id))
            .body(Body::empty())
            .unwrap();
        let response = ta.router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let photo_list: crate::models::PhotoListResponse = serde_json::from_slice(&body).unwrap();

        assert_eq!(photo_list.count, 3);
        assert_eq!(photo_list.photo_ids.len(), 3);
        assert!(photo_list.photo_ids.contains(&photo1_id));
        assert!(photo_list.photo_ids.contains(&photo2_id));
        assert!(photo_list.photo_ids.contains(&photo3_id));
    }

    #[tokio::test]
    async fn test_list_task_photos_task_not_found() {
        let ta = create_test_app().await;
        let nonexistent_id = Uuid::new_v4();

        let request = Request::get(format!("/api/tasks/{}/photos", nonexistent_id))
            .body(Body::empty())
            .unwrap();
        let response = ta.router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let error: crate::handlers::app_error::ErrorResponse = serde_json::from_slice(&body).unwrap();

        assert_eq!(error.error, "not_found");
        assert!(error.message.contains(&nonexistent_id.to_string()));
    }

    #[tokio::test]
    async fn test_list_task_photos_invalid_uuid() {
        let ta = create_test_app().await;

        let request = Request::get("/api/tasks/not-a-uuid/photos")
            .body(Body::empty())
            .unwrap();
        let response = ta.router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let error: crate::handlers::app_error::ErrorResponse = serde_json::from_slice(&body).unwrap();

        assert_eq!(error.error, "invalid_path_parameter");
    }

    #[tokio::test]
    async fn test_get_photo_by_id_found() {
        let ta = create_test_app().await;

        // Create task and photo
        let task = crate::models::Task::new("test task".to_string());
        let task_id = task.task_id;
        ta.state.task_store.create(task).await.unwrap();

        let photo = crate::models::Photo::new(task_id, "test.jpg".to_string(), 1_234_567);
        let photo_id = photo.photo_id;
        let uploaded_at = photo.uploaded_at;
        ta.state.photo_store.create(photo).await.unwrap();

        let request = Request::get(format!("/api/photos/{}", photo_id))
            .body(Body::empty())
            .unwrap();
        let response = ta.router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let photo_response: crate::models::PhotoResponse = serde_json::from_slice(&body).unwrap();

        assert_eq!(photo_response.photo_id, photo_id);
        assert_eq!(photo_response.task_id, task_id);
        assert_eq!(photo_response.filename, "test.jpg");
        assert_eq!(photo_response.size_bytes, 1_234_567);
        assert_eq!(photo_response.uploaded_at, uploaded_at);
    }

    #[tokio::test]
    async fn test_get_photo_by_id_not_found() {
        let ta = create_test_app().await;
        let nonexistent_id = Uuid::new_v4();

        let request = Request::get(format!("/api/photos/{}", nonexistent_id))
            .body(Body::empty())
            .unwrap();
        let response = ta.router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let error: crate::handlers::app_error::ErrorResponse = serde_json::from_slice(&body).unwrap();

        assert_eq!(error.error, "not_found");
        assert!(error.message.contains(&nonexistent_id.to_string()));
    }

    #[tokio::test]
    async fn test_get_photo_invalid_uuid() {
        let ta = create_test_app().await;

        let request = Request::get("/api/photos/not-a-uuid")
            .body(Body::empty())
            .unwrap();
        let response = ta.router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let error: crate::handlers::app_error::ErrorResponse = serde_json::from_slice(&body).unwrap();

        assert_eq!(error.error, "invalid_path_parameter");
    }

    #[tokio::test]
    async fn test_delete_photo_success() {
        let ta = create_test_app().await;

        // Create task and photo
        let task = crate::models::Task::new("test task".to_string());
        let task_id = task.task_id;
        ta.state.task_store.create(task).await.unwrap();

        let photo = crate::models::Photo::new(task_id, "to_delete.jpg".to_string(), 5000);
        let photo_id = photo.photo_id;
        ta.state.photo_store.create(photo).await.unwrap();

        let request = Request::delete(format!("/api/photos/{}", photo_id))
            .body(Body::empty())
            .unwrap();
        let response = ta.router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        // Verify photo is deleted
        let exists = ta.state.photo_store.exists(photo_id).await.unwrap();
        assert!(!exists);
    }

    #[tokio::test]
    async fn test_delete_photo_not_found() {
        let ta = create_test_app().await;
        let nonexistent_id = Uuid::new_v4();

        let request = Request::delete(format!("/api/photos/{}", nonexistent_id))
            .body(Body::empty())
            .unwrap();
        let response = ta.router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let error: crate::handlers::app_error::ErrorResponse = serde_json::from_slice(&body).unwrap();

        assert_eq!(error.error, "not_found");
        assert!(error.message.contains(&nonexistent_id.to_string()));
    }

    #[tokio::test]
    async fn test_delete_photo_invalid_uuid() {
        let ta = create_test_app().await;

        let request = Request::delete("/api/photos/not-a-uuid")
            .body(Body::empty())
            .unwrap();
        let response = ta.router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let error: crate::handlers::app_error::ErrorResponse = serde_json::from_slice(&body).unwrap();

        assert_eq!(error.error, "invalid_path_parameter");
    }
}
