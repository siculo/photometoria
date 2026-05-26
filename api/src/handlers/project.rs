// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 The Photometoria contributors

//! Project handlers
//!
//! This module contains handler functions for project-related endpoints.

use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::handlers::app_error::{AppError, AppJson, AppPath};
use crate::models::{
    CreateProjectRequest, Project, ProjectDetail, ProjectResponse, ProjectSummary,
    UpdateProjectRequest,
};
use crate::storage::{ActivityStore, PhotoStore, ProjectStoreError};

/// Validates and trims a project name, returning an error if empty.
fn validate_project_name(name: &str) -> Result<String, AppError> {
    let trimmed = name.trim().to_string();
    if trimmed.is_empty() {
        return Err(AppError::bad_request(
            "invalid_name",
            "Project name must not be empty",
        ));
    }
    Ok(trimmed)
}

/// Validates and trims a project context, returning an error if empty or too long.
fn validate_project_context(context: &str, max_length: usize) -> Result<String, AppError> {
    let trimmed = context.trim().to_string();
    if trimmed.is_empty() {
        return Err(AppError::bad_request(
            "invalid_context",
            "Project context must not be empty",
        ));
    }
    if trimmed.len() > max_length {
        return Err(AppError::bad_request(
            "context_too_long",
            &format!("Project context must not exceed {max_length} characters"),
        ));
    }
    Ok(trimmed)
}

/// Handler for POST /api/catalogs/{catalog_id}/tasks
///
/// Creates a new project with the provided name and context and stores it.
pub async fn create_project(
    State(state): State<AppState>,
    AppPath(catalog_id): AppPath<Uuid>,
    AppJson(request): AppJson<CreateProjectRequest>,
) -> Result<(StatusCode, Json<ProjectResponse>), AppError> {
    let name = validate_project_name(&request.name)?;
    let context =
        validate_project_context(&request.context, state.config.project.max_context_length)?;

    if state
        .project_store
        .find_by_name(catalog_id, &name)
        .await
        .unwrap_or(None)
        .is_some()
    {
        return Err(AppError::conflict(
            "name_taken",
            format!("A project with name '{}' already exists", name),
        ));
    }

    let project = Project::new(catalog_id, name, context);

    match state.project_store.create(project).await {
        Ok(created_project) => {
            let response = ProjectResponse::from(created_project);
            Ok((StatusCode::CREATED, Json(response)))
        }
        Err(ProjectStoreError::AlreadyExists(existing_project_id)) => Err(AppError::conflict(
            "already_exists",
            format!("Project with id '{}' already exists", existing_project_id),
        )),
        Err(e) => Err(AppError::internal_error(e.to_string())),
    }
}

/// Handler for GET /api/catalogs/{catalog_id}/tasks
///
/// Lists all projects in a catalog with summary information.
pub async fn list_projects(
    State(state): State<AppState>,
    AppPath(catalog_id): AppPath<Uuid>,
) -> Result<Json<Vec<ProjectSummary>>, AppError> {
    match state.project_store.list_by_catalog(catalog_id).await {
        Ok(projects) => {
            let mut summaries = Vec::with_capacity(projects.len());
            for project in projects {
                let summary =
                    get_project_summary(&state.photo_store, &state.activity_store, project).await;
                summaries.push(summary);
            }
            Ok(Json(summaries))
        }
        Err(e) => Err(AppError::internal_error(e.to_string())),
    }
}

async fn get_project_summary(
    photo_store: &Arc<dyn PhotoStore>,
    activity_store: &Arc<dyn ActivityStore>,
    project: Project,
) -> ProjectSummary {
    let photo_count = photo_store
        .count_by_task(project.project_id)
        .await
        .unwrap_or(0);
    let storage_used = photo_store
        .total_size_by_task(project.project_id)
        .await
        .unwrap_or(0);
    let job_count = activity_store
        .count_by_task(project.project_id)
        .await
        .unwrap_or(0);
    ProjectSummary {
        project_id: project.project_id,
        catalog_id: project.catalog_id,
        name: project.name,
        context: project.context,
        photo_count,
        storage_used,
        created_at: project.created_at,
        job_count,
    }
}

/// Helper function to retrieve a project and handle errors.
///
/// Returns the project if found, or an appropriate AppError otherwise.
pub(crate) async fn get_existing_project(
    project_store: &Arc<dyn crate::storage::ProjectStore>,
    project_id: Uuid,
) -> Result<Project, AppError> {
    match project_store.get(project_id).await {
        Ok(Some(project)) => Ok(project),
        Ok(None) => Err(AppError::project_not_found(project_id)),
        Err(e) => Err(AppError::internal_error(e.to_string())),
    }
}

/// Returns an error if the project has any active (Queued or Processing) activities.
///
/// Used to guard delete operations on projects and photos against in-flight processing.
pub(crate) async fn check_no_active_activities(
    activity_store: &Arc<dyn ActivityStore>,
    project_id: Uuid,
) -> Result<(), AppError> {
    match activity_store.list_by_task(project_id).await {
        Ok(activities) => {
            if activities.iter().any(|a| !a.is_finished()) {
                Err(AppError::conflict(
                    "job_active",
                    format!("Cannot delete: project '{}' has active jobs", project_id),
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
/// Retrieves detailed information about a specific project.
pub async fn get_project(
    State(state): State<AppState>,
    AppPath(project_id): AppPath<Uuid>,
) -> Result<Json<ProjectDetail>, AppError> {
    let project = get_existing_project(&state.project_store, project_id).await?;
    let detail = ProjectDetail {
        project_id: project.project_id,
        catalog_id: project.catalog_id,
        name: project.name,
        context: project.context,
        created_at: project.created_at,
        photo_count: state.photo_store.count_by_task(project_id).await.unwrap(),
        storage_used: state
            .photo_store
            .total_size_by_task(project_id)
            .await
            .unwrap(),
        job_count: state
            .activity_store
            .count_by_task(project_id)
            .await
            .unwrap_or(0),
    };
    Ok(Json(detail))
}

/// Handler for PATCH /api/tasks/{task_id}
///
/// Updates an existing project's name and context.
pub async fn update_project(
    State(state): State<AppState>,
    AppPath(project_id): AppPath<Uuid>,
    AppJson(request): AppJson<UpdateProjectRequest>,
) -> Result<Json<ProjectResponse>, AppError> {
    let project = get_existing_project(&state.project_store, project_id).await?;

    let name = validate_project_name(&request.name)?;
    let context =
        validate_project_context(&request.context, state.config.project.max_context_length)?;

    if let Some(existing) = state
        .project_store
        .find_by_name(project.catalog_id, &name)
        .await
        .unwrap_or(None)
    {
        if existing.project_id != project_id {
            return Err(AppError::conflict(
                "name_taken",
                format!("A project with name '{}' already exists", name),
            ));
        }
    }

    let updated_project = Project {
        project_id: project.project_id,
        catalog_id: project.catalog_id,
        name,
        context,
        created_at: project.created_at,
    };

    match state.project_store.update(updated_project).await {
        Ok(project) => Ok(Json(ProjectResponse::from(project))),
        Err(ProjectStoreError::NotFound(not_found_id)) => {
            Err(AppError::project_not_found(not_found_id))
        }
        Err(e) => Err(AppError::internal_error(e.to_string())),
    }
}

/// Handler for DELETE /api/tasks/{task_id}
///
/// Deletes a project and all associated data.
/// Returns 409 Conflict if the project has any active (Queued or Processing) jobs.
pub async fn delete_project(
    State(state): State<AppState>,
    AppPath(project_id): AppPath<Uuid>,
) -> Result<StatusCode, AppError> {
    check_no_active_activities(&state.activity_store, project_id).await?;
    state
        .activity_store
        .delete_by_task(project_id)
        .await
        .map_err(|e| AppError::internal_error(e.to_string()))?;
    state
        .photo_store
        .delete_by_task(project_id)
        .await
        .map_err(|e| AppError::internal_error(e.to_string()))?;
    state
        .project_store
        .delete(project_id)
        .await
        .map_err(|err| match err {
            ProjectStoreError::NotFound(not_found_id) => AppError::project_not_found(not_found_id),
            err => AppError::internal_error(err.to_string()),
        })?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::test_utils::fixtures::{create_test_state, test_catalog_id};
    use crate::models::{Activity, Photo};
    use chrono::Utc;
    use uuid::Uuid;

    #[tokio::test]
    async fn test_create_project_returns_project_response() {
        let ts = create_test_state().await;
        let request = CreateProjectRequest {
            name: "SF Vacation".to_string(),
            context: "vacation in San Francisco".to_string(),
        };

        let result = create_project(
            State(ts.state.clone()),
            AppPath(test_catalog_id()),
            AppJson(request),
        )
        .await;

        assert!(result.is_ok());
        let (status, Json(response)) = result.unwrap();
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(response.name, "SF Vacation");
        assert_eq!(response.context, "vacation in San Francisco");
        assert!(!response.project_id.is_nil());
    }

    #[tokio::test]
    async fn test_create_project_empty_context_rejected() {
        let ts = create_test_state().await;
        let request = CreateProjectRequest {
            name: "My Project".to_string(),
            context: "".to_string(),
        };

        let result = create_project(
            State(ts.state.clone()),
            AppPath(test_catalog_id()),
            AppJson(request),
        )
        .await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().body.error, "invalid_context");
    }

    #[tokio::test]
    async fn test_create_project_whitespace_context_rejected() {
        let ts = create_test_state().await;
        let request = CreateProjectRequest {
            name: "My Project".to_string(),
            context: "   ".to_string(),
        };

        let result = create_project(
            State(ts.state.clone()),
            AppPath(test_catalog_id()),
            AppJson(request),
        )
        .await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().body.error, "invalid_context");
    }

    #[tokio::test]
    async fn test_create_project_context_trimmed() {
        let ts = create_test_state().await;
        let request = CreateProjectRequest {
            name: "My Project".to_string(),
            context: "  vacation in SF  ".to_string(),
        };

        let (_, Json(response)) = create_project(
            State(ts.state.clone()),
            AppPath(test_catalog_id()),
            AppJson(request),
        )
        .await
        .unwrap();

        assert_eq!(response.context, "vacation in SF");
    }

    #[tokio::test]
    async fn test_update_project_empty_context_rejected() {
        let ts = create_test_state().await;
        let (_, Json(created)) = create_project(
            State(ts.state.clone()),
            AppPath(test_catalog_id()),
            AppJson(CreateProjectRequest {
                name: "My Project".to_string(),
                context: "original context".to_string(),
            }),
        )
        .await
        .unwrap();

        let result = update_project(
            State(ts.state.clone()),
            AppPath(created.project_id),
            AppJson(UpdateProjectRequest {
                name: "My Project".to_string(),
                context: "".to_string(),
            }),
        )
        .await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().body.error, "invalid_context");
    }

    #[tokio::test]
    async fn test_update_project_whitespace_context_rejected() {
        let ts = create_test_state().await;
        let (_, Json(created)) = create_project(
            State(ts.state.clone()),
            AppPath(test_catalog_id()),
            AppJson(CreateProjectRequest {
                name: "My Project".to_string(),
                context: "original context".to_string(),
            }),
        )
        .await
        .unwrap();

        let result = update_project(
            State(ts.state.clone()),
            AppPath(created.project_id),
            AppJson(UpdateProjectRequest {
                name: "My Project".to_string(),
                context: "   ".to_string(),
            }),
        )
        .await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().body.error, "invalid_context");
    }

    #[tokio::test]
    async fn test_create_project_empty_string_name_rejected() {
        let ts = create_test_state().await;
        let request = CreateProjectRequest {
            name: "".to_string(),
            context: "some context".to_string(),
        };

        let result = create_project(
            State(ts.state.clone()),
            AppPath(test_catalog_id()),
            AppJson(request),
        )
        .await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().body.error, "invalid_name");
    }

    #[tokio::test]
    async fn test_create_project_whitespace_name_rejected() {
        let ts = create_test_state().await;
        let request = CreateProjectRequest {
            name: "   ".to_string(),
            context: "some context".to_string(),
        };

        let result = create_project(
            State(ts.state.clone()),
            AppPath(test_catalog_id()),
            AppJson(request),
        )
        .await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().body.error, "invalid_name");
    }

    #[tokio::test]
    async fn test_create_project_name_trimmed() {
        let ts = create_test_state().await;
        let request = CreateProjectRequest {
            name: "  SF Vacation  ".to_string(),
            context: "context".to_string(),
        };

        let (_, Json(response)) = create_project(
            State(ts.state.clone()),
            AppPath(test_catalog_id()),
            AppJson(request),
        )
        .await
        .unwrap();

        assert_eq!(response.name, "SF Vacation");
    }

    #[tokio::test]
    async fn test_create_project_duplicate_name_rejected() {
        let ts = create_test_state().await;

        let request1 = CreateProjectRequest {
            name: "My Project".to_string(),
            context: "context 1".to_string(),
        };
        create_project(
            State(ts.state.clone()),
            AppPath(test_catalog_id()),
            AppJson(request1),
        )
        .await
        .unwrap();

        let request2 = CreateProjectRequest {
            name: "My Project".to_string(),
            context: "context 2".to_string(),
        };
        let result = create_project(
            State(ts.state.clone()),
            AppPath(test_catalog_id()),
            AppJson(request2),
        )
        .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.status, StatusCode::CONFLICT);
        assert_eq!(err.body.error, "name_taken");
    }

    #[tokio::test]
    async fn test_create_project_context_too_long_rejected() {
        let ts = create_test_state().await;
        let long_context = "a".repeat(ts.state.config.project.max_context_length + 1);
        let request = CreateProjectRequest {
            name: "My Project".to_string(),
            context: long_context,
        };

        let result = create_project(
            State(ts.state.clone()),
            AppPath(test_catalog_id()),
            AppJson(request),
        )
        .await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().body.error, "context_too_long");
    }

    #[tokio::test]
    async fn test_update_project_context_too_long_rejected() {
        let ts = create_test_state().await;
        let (_, Json(created)) = create_project(
            State(ts.state.clone()),
            AppPath(test_catalog_id()),
            AppJson(CreateProjectRequest {
                name: "My Project".to_string(),
                context: "original context".to_string(),
            }),
        )
        .await
        .unwrap();

        let long_context = "a".repeat(ts.state.config.project.max_context_length + 1);
        let result = update_project(
            State(ts.state.clone()),
            AppPath(created.project_id),
            AppJson(UpdateProjectRequest {
                name: "My Project".to_string(),
                context: long_context,
            }),
        )
        .await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().body.error, "context_too_long");
    }

    #[tokio::test]
    async fn test_list_projects_empty() {
        let ts = create_test_state().await;

        let result = list_projects(State(ts.state.clone()), AppPath(test_catalog_id())).await;

        assert!(result.is_ok());
        let Json(summaries) = result.unwrap();
        assert!(summaries.is_empty());
    }

    #[tokio::test]
    async fn test_list_projects_returns_created_projects() {
        let ts = create_test_state().await;

        let request1 = CreateProjectRequest {
            name: "Project 1".to_string(),
            context: "project 1".to_string(),
        };
        let request2 = CreateProjectRequest {
            name: "Project 2".to_string(),
            context: "project 2".to_string(),
        };
        let _ = create_project(
            State(ts.state.clone()),
            AppPath(test_catalog_id()),
            AppJson(request1),
        )
        .await;
        let _ = create_project(
            State(ts.state.clone()),
            AppPath(test_catalog_id()),
            AppJson(request2),
        )
        .await;

        let result = list_projects(State(ts.state.clone()), AppPath(test_catalog_id())).await;

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
    async fn test_get_project_found() {
        let ts = create_test_state().await;

        let request = CreateProjectRequest {
            name: "Test Project".to_string(),
            context: "test project".to_string(),
        };
        let (_, Json(created)) = create_project(
            State(ts.state.clone()),
            AppPath(test_catalog_id()),
            AppJson(request),
        )
        .await
        .unwrap();

        let result = get_project(State(ts.state.clone()), AppPath(created.project_id)).await;

        assert!(result.is_ok());
        let Json(detail) = result.unwrap();
        assert_eq!(detail.project_id, created.project_id);
        assert_eq!(detail.name, "Test Project");
        assert_eq!(detail.context, "test project");
        assert_eq!(detail.photo_count, 0);
        assert_eq!(detail.storage_used, 0);
        assert_eq!(detail.job_count, 0);
    }

    #[tokio::test]
    async fn test_get_project_not_found() {
        let ts = create_test_state().await;

        let project_id = Uuid::new_v4();

        let result = get_project(State(ts.state.clone()), AppPath(project_id)).await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), AppError::project_not_found(project_id));
    }

    #[tokio::test]
    async fn test_update_project_success() {
        let ts = create_test_state().await;

        let request = CreateProjectRequest {
            name: "Original Name".to_string(),
            context: "original context".to_string(),
        };
        let (_, Json(created)) = create_project(
            State(ts.state.clone()),
            AppPath(test_catalog_id()),
            AppJson(request),
        )
        .await
        .unwrap();

        let update_request = UpdateProjectRequest {
            name: "Updated Name".to_string(),
            context: "updated context".to_string(),
        };
        let result = update_project(
            State(ts.state.clone()),
            AppPath(created.project_id),
            AppJson(update_request),
        )
        .await;

        assert!(result.is_ok());
        let Json(response) = result.unwrap();
        assert_eq!(response.project_id, created.project_id);
        assert_eq!(response.name, "Updated Name");
        assert_eq!(response.context, "updated context");
        assert_eq!(response.created_at, created.created_at);
    }

    #[tokio::test]
    async fn test_update_project_same_name_allowed() {
        let ts = create_test_state().await;

        let request = CreateProjectRequest {
            name: "My Project".to_string(),
            context: "original".to_string(),
        };
        let (_, Json(created)) = create_project(
            State(ts.state.clone()),
            AppPath(test_catalog_id()),
            AppJson(request),
        )
        .await
        .unwrap();

        let update_request = UpdateProjectRequest {
            name: "My Project".to_string(),
            context: "updated context".to_string(),
        };
        let result = update_project(
            State(ts.state.clone()),
            AppPath(created.project_id),
            AppJson(update_request),
        )
        .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_update_project_duplicate_name_rejected() {
        let ts = create_test_state().await;

        create_project(
            State(ts.state.clone()),
            AppPath(test_catalog_id()),
            AppJson(CreateProjectRequest {
                name: "Project A".to_string(),
                context: "context".to_string(),
            }),
        )
        .await
        .unwrap();

        let (_, Json(project_b)) = create_project(
            State(ts.state.clone()),
            AppPath(test_catalog_id()),
            AppJson(CreateProjectRequest {
                name: "Project B".to_string(),
                context: "context".to_string(),
            }),
        )
        .await
        .unwrap();

        let result = update_project(
            State(ts.state.clone()),
            AppPath(project_b.project_id),
            AppJson(UpdateProjectRequest {
                name: "Project A".to_string(),
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
    async fn test_update_project_not_found() {
        let ts = create_test_state().await;

        let project_id = Uuid::new_v4();

        let update_request = UpdateProjectRequest {
            name: "Some Name".to_string(),
            context: "updated context".to_string(),
        };
        let result = update_project(
            State(ts.state.clone()),
            AppPath(project_id),
            AppJson(update_request),
        )
        .await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), AppError::project_not_found(project_id));
    }

    #[tokio::test]
    async fn test_delete_project_success() {
        let ts = create_test_state().await;

        let request = CreateProjectRequest {
            name: "To Delete".to_string(),
            context: "project to delete".to_string(),
        };
        let (_, Json(created)) = create_project(
            State(ts.state.clone()),
            AppPath(test_catalog_id()),
            AppJson(request),
        )
        .await
        .unwrap();

        let result = delete_project(State(ts.state.clone()), AppPath(created.project_id)).await;

        assert_eq!(result, Ok(StatusCode::NO_CONTENT));

        let get_result = get_project(State(ts.state.clone()), AppPath(created.project_id)).await;
        assert!(get_result.is_err());
        assert_eq!(
            get_result.unwrap_err(),
            AppError::project_not_found(created.project_id)
        );
    }

    #[tokio::test]
    async fn test_delete_project_not_found() {
        let ts = create_test_state().await;
        let project_id = Uuid::new_v4();

        let result = delete_project(State(ts.state.clone()), AppPath(project_id)).await;

        assert_eq!(result, Err(AppError::project_not_found(project_id)));
    }

    #[tokio::test]
    async fn test_delete_project_blocked_by_queued_job() {
        let ts = create_test_state().await;

        let (_, Json(project)) = create_project(
            State(ts.state.clone()),
            AppPath(test_catalog_id()),
            AppJson(CreateProjectRequest {
                name: "Project".to_string(),
                context: "project".to_string(),
            }),
        )
        .await
        .unwrap();

        let job = crate::models::Activity::new(
            project.project_id,
            "ollama".to_string(),
            "llava".to_string(),
            None,
            vec![],
        );
        ts.state.activity_store.create(job).await.unwrap();

        let result = delete_project(State(ts.state.clone()), AppPath(project.project_id)).await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().body.error, "job_active");
    }

    #[tokio::test]
    async fn test_delete_project_blocked_by_processing_job() {
        let ts = create_test_state().await;

        let (_, Json(project)) = create_project(
            State(ts.state.clone()),
            AppPath(test_catalog_id()),
            AppJson(CreateProjectRequest {
                name: "Project".to_string(),
                context: "project".to_string(),
            }),
        )
        .await
        .unwrap();

        let mut job = crate::models::Activity::new(
            project.project_id,
            "ollama".to_string(),
            "llava".to_string(),
            None,
            vec![],
        );
        job.start();
        ts.state.activity_store.create(job).await.unwrap();

        let result = delete_project(State(ts.state.clone()), AppPath(project.project_id)).await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().body.error, "job_active");
    }

    #[tokio::test]
    async fn test_delete_project_allowed_when_job_completed() {
        let ts = create_test_state().await;

        let (_, Json(project)) = create_project(
            State(ts.state.clone()),
            AppPath(test_catalog_id()),
            AppJson(CreateProjectRequest {
                name: "Project".to_string(),
                context: "project".to_string(),
            }),
        )
        .await
        .unwrap();

        let mut job = crate::models::Activity::new(
            project.project_id,
            "ollama".to_string(),
            "llava".to_string(),
            None,
            vec![],
        );
        job.start();
        job.complete();
        ts.state.activity_store.create(job).await.unwrap();

        let result = delete_project(State(ts.state.clone()), AppPath(project.project_id)).await;

        assert_eq!(result, Ok(StatusCode::NO_CONTENT));
    }

    #[tokio::test]
    async fn test_delete_project_allowed_when_job_cancelled() {
        let ts = create_test_state().await;

        let (_, Json(project)) = create_project(
            State(ts.state.clone()),
            AppPath(test_catalog_id()),
            AppJson(CreateProjectRequest {
                name: "Project".to_string(),
                context: "project".to_string(),
            }),
        )
        .await
        .unwrap();

        let mut job = crate::models::Activity::new(
            project.project_id,
            "ollama".to_string(),
            "llava".to_string(),
            None,
            vec![],
        );
        job.cancel();
        ts.state.activity_store.create(job).await.unwrap();

        let result = delete_project(State(ts.state.clone()), AppPath(project.project_id)).await;

        assert_eq!(result, Ok(StatusCode::NO_CONTENT));
    }

    #[tokio::test]
    async fn test_project_photo_summary() {
        let ts = create_test_state().await;
        let request = CreateProjectRequest {
            name: "SF Vacation".to_string(),
            context: "vacation in San Francisco".to_string(),
        };

        let result = create_project(
            State(ts.state.clone()),
            AppPath(test_catalog_id()),
            AppJson(request),
        )
        .await;

        assert!(result.is_ok());

        let project_id = result.unwrap().1.project_id;

        ts.state
            .photo_store
            .create(Photo {
                photo_id: Uuid::new_v4(),
                task_id: project_id,
                client_id: None,
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
                task_id: project_id,
                client_id: None,
                filename: "filename_2".to_string(),
                size_bytes: 2003800,
                uploaded_at: Utc::now(),
            })
            .await
            .unwrap();

        let result = get_project(State(ts.state.clone()), AppPath(project_id))
            .await
            .unwrap();

        assert_eq!(result.photo_count, 2);
        assert_eq!(result.storage_used, 1570000 + 2003800);
    }

    #[tokio::test]
    async fn test_list_projects_photo_summary() {
        let ts = create_test_state().await;

        let (_, Json(project1)) = create_project(
            State(ts.state.clone()),
            AppPath(test_catalog_id()),
            AppJson(CreateProjectRequest {
                name: "Project 1".to_string(),
                context: "project 1".to_string(),
            }),
        )
        .await
        .unwrap();
        let (_, Json(project2)) = create_project(
            State(ts.state.clone()),
            AppPath(test_catalog_id()),
            AppJson(CreateProjectRequest {
                name: "Project 2".to_string(),
                context: "project 2".to_string(),
            }),
        )
        .await
        .unwrap();

        ts.state
            .photo_store
            .create(Photo {
                photo_id: Uuid::new_v4(),
                task_id: project1.project_id,
                client_id: None,
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
                task_id: project1.project_id,
                client_id: None,
                filename: "photo2.jpg".to_string(),
                size_bytes: 2000000,
                uploaded_at: Utc::now(),
            })
            .await
            .unwrap();

        ts.state
            .photo_store
            .create(Photo {
                photo_id: Uuid::new_v4(),
                task_id: project2.project_id,
                client_id: None,
                filename: "photo3.jpg".to_string(),
                size_bytes: 500000,
                uploaded_at: Utc::now(),
            })
            .await
            .unwrap();

        let Json(summaries) = list_projects(State(ts.state.clone()), AppPath(test_catalog_id()))
            .await
            .unwrap();

        assert_eq!(summaries.len(), 2);

        let summary1 = summaries
            .iter()
            .find(|s| s.project_id == project1.project_id)
            .unwrap();
        assert_eq!(summary1.photo_count, 2);
        assert_eq!(summary1.storage_used, 3000000);

        let summary2 = summaries
            .iter()
            .find(|s| s.project_id == project2.project_id)
            .unwrap();
        assert_eq!(summary2.photo_count, 1);
        assert_eq!(summary2.storage_used, 500000);
    }

    #[tokio::test]
    async fn test_list_projects_job_count() {
        let ts = create_test_state().await;

        let (_, Json(project1)) = create_project(
            State(ts.state.clone()),
            AppPath(test_catalog_id()),
            AppJson(CreateProjectRequest {
                name: "Project 1".to_string(),
                context: "project 1".to_string(),
            }),
        )
        .await
        .unwrap();
        let (_, Json(project2)) = create_project(
            State(ts.state.clone()),
            AppPath(test_catalog_id()),
            AppJson(CreateProjectRequest {
                name: "Project 2".to_string(),
                context: "project 2".to_string(),
            }),
        )
        .await
        .unwrap();

        let job1 = Activity::new(
            project1.project_id,
            "ollama".to_string(),
            "llava".to_string(),
            None,
            vec![],
        );
        let job2 = Activity::new(
            project1.project_id,
            "ollama".to_string(),
            "llava".to_string(),
            None,
            vec![],
        );
        let job3 = Activity::new(
            project2.project_id,
            "ollama".to_string(),
            "llava".to_string(),
            None,
            vec![],
        );
        ts.state.activity_store.create(job1).await.unwrap();
        ts.state.activity_store.create(job2).await.unwrap();
        ts.state.activity_store.create(job3).await.unwrap();

        let Json(summaries) = list_projects(State(ts.state.clone()), AppPath(test_catalog_id()))
            .await
            .unwrap();

        let summary1 = summaries
            .iter()
            .find(|s| s.project_id == project1.project_id)
            .unwrap();
        assert_eq!(summary1.job_count, 2);

        let summary2 = summaries
            .iter()
            .find(|s| s.project_id == project2.project_id)
            .unwrap();
        assert_eq!(summary2.job_count, 1);
    }

    #[tokio::test]
    async fn test_delete_project_removes_associated_jobs() {
        let ts = create_test_state().await;

        let (_, Json(project)) = create_project(
            State(ts.state.clone()),
            AppPath(test_catalog_id()),
            AppJson(CreateProjectRequest {
                name: "Project".to_string(),
                context: "project".to_string(),
            }),
        )
        .await
        .unwrap();

        let mut activity1 = Activity::new(
            project.project_id,
            "ollama".to_string(),
            "llava".to_string(),
            None,
            vec![],
        );
        activity1.start();
        activity1.complete();
        let mut activity2 = Activity::new(
            project.project_id,
            "ollama".to_string(),
            "llava".to_string(),
            None,
            vec![],
        );
        activity2.cancel();
        ts.state.activity_store.create(activity1).await.unwrap();
        ts.state.activity_store.create(activity2).await.unwrap();

        assert_eq!(
            ts.state
                .activity_store
                .count_by_task(project.project_id)
                .await
                .unwrap(),
            2
        );

        let result = delete_project(State(ts.state.clone()), AppPath(project.project_id)).await;
        assert_eq!(result, Ok(StatusCode::NO_CONTENT));

        let remaining = ts
            .state
            .activity_store
            .count_by_task(project.project_id)
            .await
            .unwrap();
        assert_eq!(
            remaining, 0,
            "Activities should be removed when their project is deleted"
        );
    }

    #[tokio::test]
    async fn test_delete_project_removes_associated_photos() {
        let ts = create_test_state().await;

        let (_, Json(project)) = create_project(
            State(ts.state.clone()),
            AppPath(test_catalog_id()),
            AppJson(CreateProjectRequest {
                name: "Project".to_string(),
                context: "project".to_string(),
            }),
        )
        .await
        .unwrap();

        let photo1 = Photo::new(project.project_id, None, "img1.jpg".to_string(), 1_000_000);
        let photo2 = Photo::new(project.project_id, None, "img2.jpg".to_string(), 2_000_000);
        ts.state.photo_store.create(photo1).await.unwrap();
        ts.state.photo_store.create(photo2).await.unwrap();

        assert_eq!(
            ts.state
                .photo_store
                .count_by_task(project.project_id)
                .await
                .unwrap(),
            2
        );

        let result = delete_project(State(ts.state.clone()), AppPath(project.project_id)).await;
        assert_eq!(result, Ok(StatusCode::NO_CONTENT));

        let remaining = ts
            .state
            .photo_store
            .count_by_task(project.project_id)
            .await
            .unwrap();
        assert_eq!(
            remaining, 0,
            "Photos should be removed from memory when their project is deleted"
        );
    }

    #[tokio::test]
    async fn test_get_project_job_count() {
        let ts = create_test_state().await;

        let (_, Json(project)) = create_project(
            State(ts.state.clone()),
            AppPath(test_catalog_id()),
            AppJson(CreateProjectRequest {
                name: "Project".to_string(),
                context: "project".to_string(),
            }),
        )
        .await
        .unwrap();

        let job1 = Activity::new(
            project.project_id,
            "ollama".to_string(),
            "llava".to_string(),
            None,
            vec![],
        );
        let job2 = Activity::new(
            project.project_id,
            "ollama".to_string(),
            "llava".to_string(),
            None,
            vec![],
        );
        ts.state.activity_store.create(job1).await.unwrap();
        ts.state.activity_store.create(job2).await.unwrap();

        let Json(detail) = get_project(State(ts.state.clone()), AppPath(project.project_id))
            .await
            .unwrap();

        assert_eq!(detail.job_count, 2);
    }
}
