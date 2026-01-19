//! Task handlers
//!
//! This module contains handler functions for task-related endpoints.

use axum::{extract::Json, http::StatusCode};
use crate::models::{CreateTaskRequest, Task, TaskResponse};

/// Handler for POST /api/tasks
///
/// Creates a new task with the provided context.
pub async fn create_task(
    Json(request): Json<CreateTaskRequest>,
) -> (StatusCode, Json<TaskResponse>) {
    let task = Task::new(request.context);
    let response = TaskResponse::from(task);
    (StatusCode::CREATED, Json(response))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_task_returns_task_response() {
        let request = CreateTaskRequest {
            context: "vacation in San Francisco".to_string(),
        };

        let (status, Json(response)) = create_task(Json(request)).await;

        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(response.context, "vacation in San Francisco");
        assert!(!response.task_id.is_empty());
    }
}
