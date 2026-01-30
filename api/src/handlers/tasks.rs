//! Task handlers
//!
//! This module contains handler functions for task-related endpoints.

use crate::app_state::AppState;
use crate::models::{
    CreateTaskRequest, Task, TaskDetail, TaskResponse, TaskSummary, UpdateTaskRequest,
};
use crate::storage::TaskStoreError;
use axum::{
    extract::{Json, Path, State},
    http::StatusCode,
};

/// Handler for POST /api/tasks
///
/// Creates a new task with the provided context and stores it.
pub async fn create_task(
    State(state): State<AppState>,
    Json(request): Json<CreateTaskRequest>,
) -> Result<(StatusCode, Json<TaskResponse>), StatusCode> {
    let task = Task::new(request.context);

    match state.task_store.create(task).await {
        Ok(created_task) => {
            let response = TaskResponse::from(created_task);
            Ok((StatusCode::CREATED, Json(response)))
        }
        Err(TaskStoreError::AlreadyExists(_)) => Err(StatusCode::CONFLICT),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

/// Handler for GET /api/tasks
///
/// Lists all tasks with summary information.
pub async fn list_tasks(
    State(state): State<AppState>,
) -> Result<Json<Vec<TaskSummary>>, StatusCode> {
    match state.task_store.list().await {
        Ok(tasks) => {
            let summaries: Vec<TaskSummary> = tasks
                .into_iter()
                .map(|task| TaskSummary {
                    task_id: task.task_id,
                    context: task.context,
                    photo_count: 0,       // PhotoStore not implemented yet
                    storage_used_mb: 0.0, // PhotoStore not implemented yet
                    created_at: task.created_at,
                    job_count: 0, // JobStore not implemented yet
                })
                .collect();
            Ok(Json(summaries))
        }
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

/// Handler for GET /api/tasks/{task_id}
///
/// Retrieves detailed information about a specific task.
pub async fn get_task(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
) -> Result<Json<TaskDetail>, StatusCode> {
    match state.task_store.get(&task_id).await {
        Ok(Some(task)) => {
            let detail = TaskDetail {
                task_id: task.task_id,
                context: task.context,
                created_at: task.created_at,
                photo_count: 0,       // PhotoStore not implemented yet
                storage_used_mb: 0.0, // PhotoStore not implemented yet
            };
            Ok(Json(detail))
        }
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

/// Handler for PATCH /api/tasks/{task_id}
///
/// Updates an existing task's context.
pub async fn update_task(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
    Json(request): Json<UpdateTaskRequest>,
) -> Result<Json<TaskResponse>, StatusCode> {
    // First, get the existing task
    let existing_task = match state.task_store.get(&task_id).await {
        Ok(Some(task)) => task,
        Ok(None) => return Err(StatusCode::NOT_FOUND),
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    // Update the context while preserving other fields
    let updated_task = Task {
        task_id: existing_task.task_id,
        context: request.context,
        created_at: existing_task.created_at,
    };

    match state.task_store.update(updated_task).await {
        Ok(task) => Ok(Json(TaskResponse::from(task))),
        Err(TaskStoreError::NotFound(_)) => Err(StatusCode::NOT_FOUND),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

/// Handler for DELETE /api/tasks/{task_id}
///
/// Deletes a task and all associated data.
pub async fn delete_task(State(state): State<AppState>, Path(task_id): Path<String>) -> StatusCode {
    match state.task_store.delete(&task_id).await {
        Ok(()) => StatusCode::NO_CONTENT,
        Err(TaskStoreError::NotFound(_)) => StatusCode::NOT_FOUND,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::InMemoryTaskStore;
    use std::sync::Arc;

    fn create_test_state() -> AppState {
        let task_store = Arc::new(InMemoryTaskStore::new());
        AppState::new(task_store)
    }

    #[tokio::test]
    async fn test_create_task_returns_task_response() {
        let state = create_test_state();
        let request = CreateTaskRequest {
            context: "vacation in San Francisco".to_string(),
        };

        let result = create_task(State(state), Json(request)).await;

        assert!(result.is_ok());
        let (status, Json(response)) = result.unwrap();
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(response.context, "vacation in San Francisco");
        assert!(!response.task_id.is_empty());
    }

    #[tokio::test]
    async fn test_list_tasks_empty() {
        let state = create_test_state();

        let result = list_tasks(State(state)).await;

        assert!(result.is_ok());
        let Json(summaries) = result.unwrap();
        assert!(summaries.is_empty());
    }

    #[tokio::test]
    async fn test_list_tasks_returns_created_tasks() {
        let state = create_test_state();

        // Create two tasks
        let request1 = CreateTaskRequest {
            context: "task 1".to_string(),
        };
        let request2 = CreateTaskRequest {
            context: "task 2".to_string(),
        };
        let _ = create_task(State(state.clone()), Json(request1)).await;
        let _ = create_task(State(state.clone()), Json(request2)).await;

        let result = list_tasks(State(state)).await;

        assert!(result.is_ok());
        let Json(summaries) = result.unwrap();
        assert_eq!(summaries.len(), 2);
        // Check that photo_count, storage_used_mb, job_count are 0
        for summary in &summaries {
            assert_eq!(summary.photo_count, 0);
            assert_eq!(summary.storage_used_mb, 0.0);
            assert_eq!(summary.job_count, 0);
        }
    }

    #[tokio::test]
    async fn test_get_task_found() {
        let state = create_test_state();

        // Create a task first
        let request = CreateTaskRequest {
            context: "test task".to_string(),
        };
        let (_, Json(created)) = create_task(State(state.clone()), Json(request))
            .await
            .unwrap();

        let result = get_task(State(state), Path(created.task_id.clone())).await;

        assert!(result.is_ok());
        let Json(detail) = result.unwrap();
        assert_eq!(detail.task_id, created.task_id);
        assert_eq!(detail.context, "test task");
        assert_eq!(detail.photo_count, 0);
        assert_eq!(detail.storage_used_mb, 0.0);
    }

    #[tokio::test]
    async fn test_get_task_not_found() {
        let state = create_test_state();

        let result = get_task(State(state), Path("nonexistent".to_string())).await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_update_task_success() {
        let state = create_test_state();

        // Create a task first
        let request = CreateTaskRequest {
            context: "original context".to_string(),
        };
        let (_, Json(created)) = create_task(State(state.clone()), Json(request))
            .await
            .unwrap();

        // Update the task
        let update_request = UpdateTaskRequest {
            context: "updated context".to_string(),
        };
        let result = update_task(
            State(state),
            Path(created.task_id.clone()),
            Json(update_request),
        )
        .await;

        assert!(result.is_ok());
        let Json(response) = result.unwrap();
        assert_eq!(response.task_id, created.task_id);
        assert_eq!(response.context, "updated context");
        assert_eq!(response.created_at, created.created_at);
    }

    #[tokio::test]
    async fn test_update_task_not_found() {
        let state = create_test_state();

        let update_request = UpdateTaskRequest {
            context: "updated context".to_string(),
        };
        let result = update_task(
            State(state),
            Path("nonexistent".to_string()),
            Json(update_request),
        )
        .await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_delete_task_success() {
        let state = create_test_state();

        // Create a task first
        let request = CreateTaskRequest {
            context: "task to delete".to_string(),
        };
        let (_, Json(created)) = create_task(State(state.clone()), Json(request))
            .await
            .unwrap();

        // Delete the task
        let status = delete_task(State(state.clone()), Path(created.task_id.clone())).await;

        assert_eq!(status, StatusCode::NO_CONTENT);

        // Verify it's deleted
        let get_result = get_task(State(state), Path(created.task_id)).await;
        assert!(get_result.is_err());
        assert_eq!(get_result.unwrap_err(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_delete_task_not_found() {
        let state = create_test_state();

        let status = delete_task(State(state), Path("nonexistent".to_string())).await;

        assert_eq!(status, StatusCode::NOT_FOUND);
    }
}
