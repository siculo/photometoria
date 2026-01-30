use axum::{
    Router,
    routing::{get, post},
};

use crate::app_state::AppState;
use crate::handlers::tasks::{create_task, delete_task, get_task, list_tasks, update_task};

pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/version", get(version))
        .route("/api/tasks", post(create_task).get(list_tasks))
        .route(
            "/api/tasks/{task_id}",
            get(get_task).patch(update_task).delete(delete_task),
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
    use tower::ServiceExt;

    use crate::app_state::AppState;
    use crate::models::{TaskDetail, TaskResponse, TaskSummary};
    use crate::storage::{InMemoryTaskStore, TaskStore};

    fn create_test_app() -> (Router, AppState) {
        let task_store: Arc<dyn TaskStore> = Arc::new(InMemoryTaskStore::new());
        let state = AppState::new(task_store);
        let app = create_router(state.clone());
        (app, state)
    }

    #[tokio::test]
    async fn test_version_returns_package_version() {
        let (app, _) = create_test_app();
        let request = Request::get("/version").body(Body::empty()).unwrap();
        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), 200);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let body_str = String::from_utf8(body.to_vec()).unwrap();

        assert_eq!(body_str, env!("CARGO_PKG_VERSION"));
    }

    #[tokio::test]
    async fn test_post_tasks_creates_task() {
        let (app, _) = create_test_app();
        let request = Request::post("/api/tasks")
            .header("Content-Type", "application/json")
            .body(Body::from(json!({"context": "test context"}).to_string()))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::CREATED);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let task_response: TaskResponse = serde_json::from_slice(&body).unwrap();

        assert_eq!(task_response.context, "test context");
        assert!(!task_response.task_id.is_empty());
    }

    #[tokio::test]
    async fn test_get_tasks_returns_empty_list() {
        let (app, _) = create_test_app();
        let request = Request::get("/api/tasks").body(Body::empty()).unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let summaries: Vec<TaskSummary> = serde_json::from_slice(&body).unwrap();

        assert!(summaries.is_empty());
    }

    #[tokio::test]
    async fn test_get_tasks_returns_created_tasks() {
        let (app, state) = create_test_app();

        // Create a task first using the store directly
        let task = crate::models::Task::new("test task".to_string());
        let task_id = task.task_id.clone();
        state.task_store.create(task).await.unwrap();

        let request = Request::get("/api/tasks").body(Body::empty()).unwrap();
        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let summaries: Vec<TaskSummary> = serde_json::from_slice(&body).unwrap();

        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].task_id, task_id);
        assert_eq!(summaries[0].context, "test task");
    }

    #[tokio::test]
    async fn test_get_task_by_id_found() {
        let (app, state) = create_test_app();

        // Create a task first
        let task = crate::models::Task::new("test task".to_string());
        let task_id = task.task_id.clone();
        state.task_store.create(task).await.unwrap();

        let request = Request::get(format!("/api/tasks/{}", task_id))
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let detail: TaskDetail = serde_json::from_slice(&body).unwrap();

        assert_eq!(detail.task_id, task_id);
        assert_eq!(detail.context, "test task");
    }

    #[tokio::test]
    async fn test_get_task_by_id_not_found() {
        let (app, _) = create_test_app();

        let request = Request::get("/api/tasks/nonexistent")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_patch_task_updates_context() {
        let (app, state) = create_test_app();

        // Create a task first
        let task = crate::models::Task::new("original context".to_string());
        let task_id = task.task_id.clone();
        state.task_store.create(task).await.unwrap();

        let request = Request::patch(format!("/api/tasks/{}", task_id))
            .header("Content-Type", "application/json")
            .body(Body::from(
                json!({"context": "updated context"}).to_string(),
            ))
            .unwrap();
        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let task_response: TaskResponse = serde_json::from_slice(&body).unwrap();

        assert_eq!(task_response.task_id, task_id);
        assert_eq!(task_response.context, "updated context");
    }

    #[tokio::test]
    async fn test_patch_task_not_found() {
        let (app, _) = create_test_app();

        let request = Request::patch("/api/tasks/nonexistent")
            .header("Content-Type", "application/json")
            .body(Body::from(
                json!({"context": "updated context"}).to_string(),
            ))
            .unwrap();
        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_delete_task_success() {
        let (app, state) = create_test_app();

        // Create a task first
        let task = crate::models::Task::new("task to delete".to_string());
        let task_id = task.task_id.clone();
        state.task_store.create(task).await.unwrap();

        let request = Request::delete(format!("/api/tasks/{}", task_id))
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        // Verify it's deleted
        let exists = state.task_store.exists(&task_id).await.unwrap();
        assert!(!exists);
    }

    #[tokio::test]
    async fn test_delete_task_not_found() {
        let (app, _) = create_test_app();

        let request = Request::delete("/api/tasks/nonexistent")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
