// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 The Photometoria contributors

//! Task handlers
//!
//! This module contains handler functions for task-related endpoints.

use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::handlers::app_error::{AppError, AppJson, AppPath};
use crate::models::{
    CreateTaskRequest, Task, TaskDetail, TaskResponse, TaskSummary, UpdateTaskRequest,
};
use crate::storage::{JobStore, PhotoStore, TaskStoreError};

/// Validates and trims a task name, returning an error if empty.
fn validate_task_name(name: &str) -> Result<String, AppError> {
    let trimmed = name.trim().to_string();
    if trimmed.is_empty() {
        return Err(AppError::bad_request(
            "invalid_name",
            "Task name must not be empty",
        ));
    }
    Ok(trimmed)
}

/// Handler for POST /api/tasks
///
/// Creates a new task with the provided name and context and stores it.
pub async fn create_task(
    State(state): State<AppState>,
    AppJson(request): AppJson<CreateTaskRequest>,
) -> Result<(StatusCode, Json<TaskResponse>), AppError> {
    let name = validate_task_name(&request.name)?;

    if state
        .task_store
        .find_by_name(&name)
        .await
        .unwrap_or(None)
        .is_some()
    {
        return Err(AppError::conflict(
            "name_taken",
            format!("A task with name '{}' already exists", name),
        ));
    }

    let task = Task::new(name, request.context);

    match state.task_store.create(task).await {
        Ok(created_task) => {
            let response = TaskResponse::from(created_task);
            Ok((StatusCode::CREATED, Json(response)))
        }
        Err(TaskStoreError::AlreadyExists(existing_task_id)) => Err(AppError::conflict(
            "already_exists",
            format!("Task with id '{}' already exists", existing_task_id),
        )),
        Err(e) => Err(AppError::internal_error(e.to_string())),
    }
}

/// Handler for GET /api/tasks
///
/// Lists all tasks with summary information.
pub async fn list_tasks(State(state): State<AppState>) -> Result<Json<Vec<TaskSummary>>, AppError> {
    match state.task_store.list().await {
        Ok(tasks) => {
            let mut summaries = Vec::with_capacity(tasks.len());
            let photo_store = &state.photo_store;
            for task in tasks {
                let summary = get_task_summary(photo_store, task).await;
                summaries.push(summary);
            }
            Ok(Json(summaries))
        }
        Err(e) => Err(AppError::internal_error(e.to_string())),
    }
}

async fn get_task_summary(photo_store: &Arc<dyn PhotoStore>, task: Task) -> TaskSummary {
    let photo_count = photo_store.count_by_task(task.task_id).await.unwrap_or(0);
    let storage_used = photo_store
        .total_size_by_task(task.task_id)
        .await
        .unwrap_or(0); // TODO: è corretto che in caso di errore il risultato sia zero?
    TaskSummary {
        task_id: task.task_id,
        name: task.name,
        context: task.context,
        photo_count,
        storage_used,
        created_at: task.created_at,
        job_count: 0, // JobStore not implemented yet
    }
}

/// Helper function to retrieve a task and handle errors.
///
/// Returns the task if found, or an appropriate AppError otherwise.
pub(crate) async fn get_existing_task(
    task_store: &Arc<dyn crate::storage::TaskStore>,
    task_id: Uuid,
) -> Result<Task, AppError> {
    match task_store.get(task_id).await {
        Ok(Some(task)) => Ok(task),
        Ok(None) => Err(AppError::task_not_found(task_id)),
        Err(e) => Err(AppError::internal_error(e.to_string())),
    }
}

/// Returns an error if the task has any active (Queued or Processing) jobs.
///
/// Used to guard delete operations on tasks and photos against in-flight processing.
pub(crate) async fn check_no_active_jobs(
    job_store: &Arc<dyn JobStore>,
    task_id: Uuid,
) -> Result<(), AppError> {
    match job_store.list_by_task(task_id).await {
        Ok(jobs) => {
            if jobs.iter().any(|j| !j.is_finished()) {
                Err(AppError::conflict(
                    "job_active",
                    format!("Cannot delete: task '{}' has active jobs", task_id),
                ))
            } else {
                Ok(())
            }
        }
        Err(e) => Err(AppError::internal_error(e.to_string())),
    }
}

/// Handler for GET /api/tasks/{task_id}
///
/// Retrieves detailed information about a specific task.
pub async fn get_task(
    State(state): State<AppState>,
    AppPath(task_id): AppPath<Uuid>,
) -> Result<Json<TaskDetail>, AppError> {
    let task = get_existing_task(&state.task_store, task_id).await?;
    let detail = TaskDetail {
        task_id: task.task_id,
        name: task.name,
        context: task.context,
        created_at: task.created_at,
        photo_count: state.photo_store.count_by_task(task_id).await.unwrap(),
        storage_used: state.photo_store.total_size_by_task(task_id).await.unwrap(),
    };
    Ok(Json(detail))
}

/// Handler for PATCH /api/tasks/{task_id}
///
/// Updates an existing task's name and context.
pub async fn update_task(
    State(state): State<AppState>,
    AppPath(task_id): AppPath<Uuid>,
    AppJson(request): AppJson<UpdateTaskRequest>,
) -> Result<Json<TaskResponse>, AppError> {
    let task = get_existing_task(&state.task_store, task_id).await?;

    let name = validate_task_name(&request.name)?;

    if let Some(existing) = state.task_store.find_by_name(&name).await.unwrap_or(None) {
        if existing.task_id != task_id {
            return Err(AppError::conflict(
                "name_taken",
                format!("A task with name '{}' already exists", name),
            ));
        }
    }

    let updated_task = Task {
        task_id: task.task_id,
        name,
        context: request.context,
        created_at: task.created_at,
    };

    match state.task_store.update(updated_task).await {
        Ok(task) => Ok(Json(TaskResponse::from(task))),
        Err(TaskStoreError::NotFound(not_found_task_id)) => {
            Err(AppError::task_not_found(not_found_task_id))
        }
        Err(e) => Err(AppError::internal_error(e.to_string())),
    }
}

/// Handler for DELETE /api/tasks/{task_id}
///
/// Deletes a task and all associated data.
/// Returns 409 Conflict if the task has any active (Queued or Processing) jobs.
pub async fn delete_task(
    State(state): State<AppState>,
    AppPath(task_id): AppPath<Uuid>,
) -> Result<StatusCode, AppError> {
    check_no_active_jobs(&state.job_store, task_id).await?;
    match state.task_store.delete(task_id).await {
        Ok(()) => Ok(StatusCode::NO_CONTENT),
        Err(TaskStoreError::NotFound(not_found_task_id)) => {
            Err(AppError::task_not_found(not_found_task_id))
        }
        Err(e) => Err(AppError::internal_error(e.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::test_utils::fixtures::create_test_state;
    use crate::models::Photo;
    use chrono::Utc;
    use uuid::Uuid;

    #[tokio::test]
    async fn test_create_task_returns_task_response() {
        let ts = create_test_state().await;
        let request = CreateTaskRequest {
            name: "SF Vacation".to_string(),
            context: "vacation in San Francisco".to_string(),
        };

        let result = create_task(State(ts.state.clone()), AppJson(request)).await;

        assert!(result.is_ok());
        let (status, Json(response)) = result.unwrap();
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(response.name, "SF Vacation");
        assert_eq!(response.context, "vacation in San Francisco");
        assert!(!response.task_id.is_nil());
    }

    #[tokio::test]
    async fn test_create_task_empty_string_name_rejected() {
        let ts = create_test_state().await;
        let request = CreateTaskRequest {
            name: "".to_string(),
            context: "some context".to_string(),
        };

        let result = create_task(State(ts.state.clone()), AppJson(request)).await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().body.error, "invalid_name");
    }

    #[tokio::test]
    async fn test_create_task_whitespace_name_rejected() {
        let ts = create_test_state().await;
        let request = CreateTaskRequest {
            name: "   ".to_string(),
            context: "some context".to_string(),
        };

        let result = create_task(State(ts.state.clone()), AppJson(request)).await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().body.error, "invalid_name");
    }

    #[tokio::test]
    async fn test_create_task_name_trimmed() {
        let ts = create_test_state().await;
        let request = CreateTaskRequest {
            name: "  SF Vacation  ".to_string(),
            context: "context".to_string(),
        };

        let (_, Json(response)) = create_task(State(ts.state.clone()), AppJson(request))
            .await
            .unwrap();

        assert_eq!(response.name, "SF Vacation");
    }

    #[tokio::test]
    async fn test_create_task_duplicate_name_rejected() {
        let ts = create_test_state().await;

        let request1 = CreateTaskRequest {
            name: "My Task".to_string(),
            context: "context 1".to_string(),
        };
        create_task(State(ts.state.clone()), AppJson(request1))
            .await
            .unwrap();

        let request2 = CreateTaskRequest {
            name: "My Task".to_string(),
            context: "context 2".to_string(),
        };
        let result = create_task(State(ts.state.clone()), AppJson(request2)).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.status, StatusCode::CONFLICT);
        assert_eq!(err.body.error, "name_taken");
    }

    #[tokio::test]
    async fn test_list_tasks_empty() {
        let ts = create_test_state().await;

        let result = list_tasks(State(ts.state.clone())).await;

        assert!(result.is_ok());
        let Json(summaries) = result.unwrap();
        assert!(summaries.is_empty());
    }

    #[tokio::test]
    async fn test_list_tasks_returns_created_tasks() {
        let ts = create_test_state().await;

        let request1 = CreateTaskRequest {
            name: "Task 1".to_string(),
            context: "task 1".to_string(),
        };
        let request2 = CreateTaskRequest {
            name: "Task 2".to_string(),
            context: "task 2".to_string(),
        };
        let _ = create_task(State(ts.state.clone()), AppJson(request1)).await;
        let _ = create_task(State(ts.state.clone()), AppJson(request2)).await;

        let result = list_tasks(State(ts.state.clone())).await;

        assert!(result.is_ok());
        let Json(summaries) = result.unwrap();
        assert_eq!(summaries.len(), 2);
        for summary in &summaries {
            assert_eq!(summary.photo_count, 0);
            assert_eq!(summary.storage_used, 0);
            assert_eq!(summary.job_count, 0);
        }
    }

    #[tokio::test]
    async fn test_get_task_found() {
        let ts = create_test_state().await;

        let request = CreateTaskRequest {
            name: "Test Task".to_string(),
            context: "test task".to_string(),
        };
        let (_, Json(created)) = create_task(State(ts.state.clone()), AppJson(request))
            .await
            .unwrap();

        let result = get_task(State(ts.state.clone()), AppPath(created.task_id)).await;

        assert!(result.is_ok());
        let Json(detail) = result.unwrap();
        assert_eq!(detail.task_id, created.task_id);
        assert_eq!(detail.name, "Test Task");
        assert_eq!(detail.context, "test task");
        assert_eq!(detail.photo_count, 0);
        assert_eq!(detail.storage_used, 0);
    }

    #[tokio::test]
    async fn test_get_task_not_found() {
        let ts = create_test_state().await;

        let task_id = Uuid::new_v4();

        let result = get_task(State(ts.state.clone()), AppPath(task_id)).await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), AppError::task_not_found(task_id));
    }

    #[tokio::test]
    async fn test_update_task_success() {
        let ts = create_test_state().await;

        let request = CreateTaskRequest {
            name: "Original Name".to_string(),
            context: "original context".to_string(),
        };
        let (_, Json(created)) = create_task(State(ts.state.clone()), AppJson(request))
            .await
            .unwrap();

        let update_request = UpdateTaskRequest {
            name: "Updated Name".to_string(),
            context: "updated context".to_string(),
        };
        let result = update_task(
            State(ts.state.clone()),
            AppPath(created.task_id),
            AppJson(update_request),
        )
        .await;

        assert!(result.is_ok());
        let Json(response) = result.unwrap();
        assert_eq!(response.task_id, created.task_id);
        assert_eq!(response.name, "Updated Name");
        assert_eq!(response.context, "updated context");
        assert_eq!(response.created_at, created.created_at);
    }

    #[tokio::test]
    async fn test_update_task_same_name_allowed() {
        let ts = create_test_state().await;

        let request = CreateTaskRequest {
            name: "My Task".to_string(),
            context: "original".to_string(),
        };
        let (_, Json(created)) = create_task(State(ts.state.clone()), AppJson(request))
            .await
            .unwrap();

        let update_request = UpdateTaskRequest {
            name: "My Task".to_string(),
            context: "updated context".to_string(),
        };
        let result = update_task(
            State(ts.state.clone()),
            AppPath(created.task_id),
            AppJson(update_request),
        )
        .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_update_task_duplicate_name_rejected() {
        let ts = create_test_state().await;

        create_task(
            State(ts.state.clone()),
            AppJson(CreateTaskRequest {
                name: "Task A".to_string(),
                context: "context".to_string(),
            }),
        )
        .await
        .unwrap();

        let (_, Json(task_b)) = create_task(
            State(ts.state.clone()),
            AppJson(CreateTaskRequest {
                name: "Task B".to_string(),
                context: "context".to_string(),
            }),
        )
        .await
        .unwrap();

        let result = update_task(
            State(ts.state.clone()),
            AppPath(task_b.task_id),
            AppJson(UpdateTaskRequest {
                name: "Task A".to_string(),
                context: "context".to_string(),
            }),
        )
        .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.status, StatusCode::CONFLICT);
        assert_eq!(err.body.error, "name_taken");
    }

    #[tokio::test]
    async fn test_update_task_not_found() {
        let ts = create_test_state().await;

        let task_id = Uuid::new_v4();

        let update_request = UpdateTaskRequest {
            name: "Some Name".to_string(),
            context: "updated context".to_string(),
        };
        let result = update_task(
            State(ts.state.clone()),
            AppPath(task_id),
            AppJson(update_request),
        )
        .await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), AppError::task_not_found(task_id));
    }

    #[tokio::test]
    async fn test_delete_task_success() {
        let ts = create_test_state().await;

        let request = CreateTaskRequest {
            name: "To Delete".to_string(),
            context: "task to delete".to_string(),
        };
        let (_, Json(created)) = create_task(State(ts.state.clone()), AppJson(request))
            .await
            .unwrap();

        let result = delete_task(State(ts.state.clone()), AppPath(created.task_id)).await;

        assert_eq!(result, Ok(StatusCode::NO_CONTENT));

        let get_result = get_task(State(ts.state.clone()), AppPath(created.task_id)).await;
        assert!(get_result.is_err());
        assert_eq!(
            get_result.unwrap_err(),
            AppError::task_not_found(created.task_id)
        );
    }

    #[tokio::test]
    async fn test_delete_task_not_found() {
        let ts = create_test_state().await;
        let task_id = Uuid::new_v4();

        let result = delete_task(State(ts.state.clone()), AppPath(task_id)).await;

        assert_eq!(result, Err(AppError::task_not_found(task_id)));
    }

    #[tokio::test]
    async fn test_delete_task_blocked_by_queued_job() {
        let ts = create_test_state().await;

        let (_, Json(task)) = create_task(
            State(ts.state.clone()),
            AppJson(CreateTaskRequest {
                name: "Task".to_string(),
                context: "task".to_string(),
            }),
        )
        .await
        .unwrap();

        let job = crate::models::Job::new(task.task_id, "llava".to_string(), vec![]);
        ts.state.job_store.create(job).await.unwrap();

        let result = delete_task(State(ts.state.clone()), AppPath(task.task_id)).await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().body.error, "job_active");
    }

    #[tokio::test]
    async fn test_delete_task_blocked_by_processing_job() {
        let ts = create_test_state().await;

        let (_, Json(task)) = create_task(
            State(ts.state.clone()),
            AppJson(CreateTaskRequest {
                name: "Task".to_string(),
                context: "task".to_string(),
            }),
        )
        .await
        .unwrap();

        let mut job = crate::models::Job::new(task.task_id, "llava".to_string(), vec![]);
        job.start();
        ts.state.job_store.create(job).await.unwrap();

        let result = delete_task(State(ts.state.clone()), AppPath(task.task_id)).await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().body.error, "job_active");
    }

    #[tokio::test]
    async fn test_delete_task_allowed_when_job_completed() {
        let ts = create_test_state().await;

        let (_, Json(task)) = create_task(
            State(ts.state.clone()),
            AppJson(CreateTaskRequest {
                name: "Task".to_string(),
                context: "task".to_string(),
            }),
        )
        .await
        .unwrap();

        let mut job = crate::models::Job::new(task.task_id, "llava".to_string(), vec![]);
        job.start();
        job.complete();
        ts.state.job_store.create(job).await.unwrap();

        let result = delete_task(State(ts.state.clone()), AppPath(task.task_id)).await;

        assert_eq!(result, Ok(StatusCode::NO_CONTENT));
    }

    #[tokio::test]
    async fn test_delete_task_allowed_when_job_cancelled() {
        let ts = create_test_state().await;

        let (_, Json(task)) = create_task(
            State(ts.state.clone()),
            AppJson(CreateTaskRequest {
                name: "Task".to_string(),
                context: "task".to_string(),
            }),
        )
        .await
        .unwrap();

        let mut job = crate::models::Job::new(task.task_id, "llava".to_string(), vec![]);
        job.cancel();
        ts.state.job_store.create(job).await.unwrap();

        let result = delete_task(State(ts.state.clone()), AppPath(task.task_id)).await;

        assert_eq!(result, Ok(StatusCode::NO_CONTENT));
    }

    #[tokio::test]
    async fn test_task_photo_summary() {
        let ts = create_test_state().await;
        let request = CreateTaskRequest {
            name: "SF Vacation".to_string(),
            context: "vacation in San Francisco".to_string(),
        };

        let result = create_task(State(ts.state.clone()), AppJson(request)).await;

        assert!(result.is_ok());

        let task_id = result.unwrap().1.task_id;

        ts.state
            .photo_store
            .create(Photo {
                photo_id: Uuid::new_v4(),
                task_id,
                filename: "filename_1".to_string(),
                size_bytes: 1570000,
                uploaded_at: Utc::now(),
            })
            .await
            .unwrap();
        ts.state
            .photo_store
            .create(Photo {
                photo_id: Uuid::new_v4(),
                task_id,
                filename: "filename_2".to_string(),
                size_bytes: 2003800,
                uploaded_at: Utc::now(),
            })
            .await
            .unwrap();

        let result = get_task(State(ts.state.clone()), AppPath(task_id))
            .await
            .unwrap();

        assert_eq!(result.photo_count, 2);
        assert_eq!(result.storage_used, 1570000 + 2003800);
    }

    #[tokio::test]
    async fn test_list_tasks_photo_summary() {
        let ts = create_test_state().await;

        // Create two tasks
        let (_, Json(task1)) = create_task(
            State(ts.state.clone()),
            AppJson(CreateTaskRequest {
                name: "Task 1".to_string(),
                context: "task 1".to_string(),
            }),
        )
        .await
        .unwrap();
        let (_, Json(task2)) = create_task(
            State(ts.state.clone()),
            AppJson(CreateTaskRequest {
                name: "Task 2".to_string(),
                context: "task 2".to_string(),
            }),
        )
        .await
        .unwrap();

        // Add photos to task 1
        ts.state
            .photo_store
            .create(Photo {
                photo_id: Uuid::new_v4(),
                task_id: task1.task_id,
                filename: "photo1.jpg".to_string(),
                size_bytes: 1000000,
                uploaded_at: Utc::now(),
            })
            .await
            .unwrap();
        ts.state
            .photo_store
            .create(Photo {
                photo_id: Uuid::new_v4(),
                task_id: task1.task_id,
                filename: "photo2.jpg".to_string(),
                size_bytes: 2000000,
                uploaded_at: Utc::now(),
            })
            .await
            .unwrap();

        // Add one photo to task 2
        ts.state
            .photo_store
            .create(Photo {
                photo_id: Uuid::new_v4(),
                task_id: task2.task_id,
                filename: "photo3.jpg".to_string(),
                size_bytes: 500000,
                uploaded_at: Utc::now(),
            })
            .await
            .unwrap();

        let Json(summaries) = list_tasks(State(ts.state.clone())).await.unwrap();

        assert_eq!(summaries.len(), 2);

        let summary1 = summaries
            .iter()
            .find(|s| s.task_id == task1.task_id)
            .unwrap();
        assert_eq!(summary1.photo_count, 2);
        assert_eq!(summary1.storage_used, 3000000);

        let summary2 = summaries
            .iter()
            .find(|s| s.task_id == task2.task_id)
            .unwrap();
        assert_eq!(summary2.photo_count, 1);
        assert_eq!(summary2.storage_used, 500000);
    }
}
