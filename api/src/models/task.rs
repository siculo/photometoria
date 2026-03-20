// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 The Photometoria contributors

//! Task data model and related DTOs
//!
//! This module defines the Task entity and all related Data Transfer Objects (DTOs)
//! for the REST API endpoints.
//!
//! A Task represents a working session for a photographer, containing uploaded photos
//! and shared context hints for AI analysis.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ============================================================================
// Task Entity (Internal Domain Model)
// ============================================================================

/// Represents a working session for a photographer.
///
/// A Task is a container for uploaded photos and shared context hints.
/// It's short-lived (one working session) but has no automatic timeout initially.
///
/// ## Lifecycle
/// ```text
/// Created → Photos Uploaded → Jobs Created/Executed → Explicitly Deleted
/// ```
///
/// ## Current Limitation
/// Only one active task allowed at a time (returns error if another exists).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    /// Unique identifier (UUID)
    pub task_id: Uuid,

    /// Identifier of the Lightroom catalog this task belongs to
    ///
    /// Defaults to nil UUID for backward compatibility with existing
    /// storage that predates this field.
    #[serde(default)]
    pub catalog_id: Uuid,

    /// Short human-readable name for the task (must be unique)
    ///
    /// Defaults to empty string for backward compatibility with existing
    /// storage that predates this field.
    #[serde(default)]
    pub name: String,

    /// User-provided context information for AI analysis
    ///
    /// This context is used by the AI models to better understand the photos
    /// being analyzed. For example: "vacation in San Francisco, summer 2024"
    ///
    /// TODO: Add max length validation (e.g., 1000-5000 chars)
    pub context: String,

    /// Timestamp when the task was created (ISO 8601)
    pub created_at: DateTime<Utc>,
}

impl Task {
    /// Creates a new Task with generated UUID and current timestamp.
    ///
    /// # Arguments
    /// * `catalog_id` - UUID of the Lightroom catalog this task belongs to
    /// * `name` - Short human-readable name for the task
    /// * `context` - User-provided context information for AI analysis
    ///
    /// # Example
    /// ```
    /// use photometoria_rest_api::models::Task;
    /// use uuid::Uuid;
    ///
    /// let catalog_id = Uuid::new_v4();
    /// let task = Task::new(catalog_id, "SF Vacation".to_string(), "vacation in San Francisco".to_string());
    /// ```
    pub fn new(catalog_id: Uuid, name: String, context: String) -> Self {
        Self {
            task_id: Uuid::new_v4(),
            catalog_id,
            name,
            context,
            created_at: Utc::now(),
        }
    }

    /// Assigns a generated name if the current name is empty.
    ///
    /// Used for backward compatibility when loading tasks from storage
    /// that predate the `name` field. Generates a name like "Untitled Task a1b2"
    /// using the first 4 hex characters of the task ID.
    pub fn ensure_name(&mut self) {
        if self.name.is_empty() {
            let short_id = &self.task_id.to_string()[..4];
            self.name = format!("Untitled Task {}", short_id);
        }
    }
}

// ============================================================================
// DTOs for API Endpoints
// ============================================================================

/// Request body for creating a new task.
///
/// Used by: `POST /api/tasks`
///
/// # Example JSON
/// ```json
/// {
///   "name": "SF Vacation 2024",
///   "context": "vacation in San Francisco, summer 2024"
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTaskRequest {
    /// Short human-readable name for the task (must be unique)
    pub name: String,

    /// User-provided context information for AI analysis
    pub context: String,
}

/// Response for task creation and basic task information.
///
/// Used by:
/// - `POST /api/tasks` (creation response)
/// - `GET /api/tasks/{task_id}` (basic info)
///
/// # Example JSON
/// ```json
/// {
///   "task_id": "550e8400-e29b-41d4-a716-446655440000",
///   "catalog_id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
///   "name": "SF Vacation 2024",
///   "context": "vacation in San Francisco, summer 2024",
///   "created_at": "2024-01-15T10:30:00Z"
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResponse {
    /// Unique task identifier
    pub task_id: Uuid,

    /// Identifier of the Lightroom catalog this task belongs to
    pub catalog_id: Uuid,

    /// Short human-readable name for the task
    pub name: String,

    /// User-provided context information
    pub context: String,

    /// Task creation timestamp (ISO 8601)
    pub created_at: DateTime<Utc>,
}

/// Summary information about a task for listing endpoints.
///
/// Used by: `GET /api/tasks` (list all tasks)
///
/// Includes aggregated information from photos and jobs.
///
/// # Example JSON
/// ```json
/// {
///   "task_id": "550e8400-e29b-41d4-a716-446655440000",
///   "catalog_id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
///   "name": "SF Vacation 2024",
///   "context": "vacation in SF",
///   "photo_count": 15,
///   "storage_used": 243434374,
///   "created_at": "2024-01-15T10:30:00Z",
///   "job_count": 2
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSummary {
    /// Unique task identifier
    pub task_id: Uuid,

    /// Identifier of the Lightroom catalog this task belongs to
    pub catalog_id: Uuid,

    /// Short human-readable name for the task
    pub name: String,

    /// User-provided context information
    pub context: String,

    /// Number of photos uploaded to this task
    pub photo_count: usize,

    /// Total storage used by photos in this task in bytes
    pub storage_used: u64,

    /// Task creation timestamp (ISO 8601)
    pub created_at: DateTime<Utc>,

    /// Number of jobs associated with this task
    pub job_count: usize,
}

/// Detailed information about a task including aggregated data.
///
/// Used by: `GET /api/tasks/{task_id}` (detailed view)
///
/// # Example JSON
/// ```json
/// {
///   "task_id": "550e8400-e29b-41d4-a716-446655440000",
///   "catalog_id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
///   "name": "SF Vacation 2024",
///   "context": "vacation in SF",
///   "created_at": "2024-01-15T10:30:00Z",
///   "photo_count": 15,
///   "storage_used": 42378436,
///   "job_count": 2
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDetail {
    /// Unique task identifier
    pub task_id: Uuid,

    /// Identifier of the Lightroom catalog this task belongs to
    pub catalog_id: Uuid,

    /// Short human-readable name for the task
    pub name: String,

    /// User-provided context information
    pub context: String,

    /// Task creation timestamp (ISO 8601)
    pub created_at: DateTime<Utc>,

    /// Number of photos uploaded to this task
    pub photo_count: usize,

    /// Total storage used by photos in this task in bytes
    pub storage_used: u64,

    /// Number of jobs associated with this task
    pub job_count: usize,
}

/// Request body for updating an existing task.
///
/// Used by: `PATCH /api/tasks/{task_id}`
///
/// # Example JSON
/// ```json
/// {
///   "name": "Updated Name",
///   "context": "updated context information"
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateTaskRequest {
    /// Updated task name (must be unique)
    pub name: String,

    /// Updated context information
    pub context: String,
}

// ============================================================================
// Conversions from Entity to DTOs
// ============================================================================

impl From<Task> for TaskResponse {
    /// Converts a Task entity into a TaskResponse DTO.
    ///
    /// This is a simple 1:1 mapping of fields.
    fn from(task: Task) -> Self {
        Self {
            task_id: task.task_id,
            catalog_id: task.catalog_id,
            name: task.name,
            context: task.context,
            created_at: task.created_at,
        }
    }
}

impl From<&Task> for TaskResponse {
    /// Converts a Task reference into a TaskResponse DTO.
    ///
    /// This is a simple 1:1 mapping of fields.
    fn from(task: &Task) -> Self {
        Self {
            task_id: task.task_id,
            catalog_id: task.catalog_id,
            name: task.name.clone(),
            context: task.context.clone(),
            created_at: task.created_at,
        }
    }
}

// Note: TaskSummary and TaskDetail require additional data from PhotoStore
// and JobStore, so they don't have a simple From<Task> implementation.
// They will be constructed in the handler layer where all necessary data
// is available.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_new_generates_uuid() {
        let catalog_id = Uuid::new_v4();
        let task = Task::new(catalog_id, "Test".to_string(), "test context".to_string());
        assert!(!task.task_id.is_nil());
    }

    #[test]
    fn test_task_new_sets_fields() {
        let catalog_id = Uuid::new_v4();
        let task = Task::new(
            catalog_id,
            "SF Vacation".to_string(),
            "vacation in SF".to_string(),
        );
        assert_eq!(task.catalog_id, catalog_id);
        assert_eq!(task.name, "SF Vacation");
        assert_eq!(task.context, "vacation in SF");
    }

    #[test]
    fn test_task_to_response_conversion() {
        let catalog_id = Uuid::new_v4();
        let task = Task::new(catalog_id, "Test".to_string(), "test".to_string());
        let response: TaskResponse = (&task).into();

        assert_eq!(response.task_id, task.task_id);
        assert_eq!(response.catalog_id, task.catalog_id);
        assert_eq!(response.name, task.name);
        assert_eq!(response.context, task.context);
        assert_eq!(response.created_at, task.created_at);
    }

    #[test]
    fn test_task_serialization() {
        let task = Task::new(
            Uuid::new_v4(),
            "Test".to_string(),
            "test context".to_string(),
        );
        let json = serde_json::to_string(&task).unwrap();

        assert!(json.contains("task_id"));
        assert!(json.contains("catalog_id"));
        assert!(json.contains("name"));
        assert!(json.contains("context"));
        assert!(json.contains("created_at"));
    }

    #[test]
    fn test_create_task_request_deserialization() {
        let json = r#"{"name":"SF Vacation","context":"vacation in SF"}"#;
        let request: CreateTaskRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.name, "SF Vacation");
        assert_eq!(request.context, "vacation in SF");
    }

    #[test]
    fn test_task_deserialization_without_name_and_catalog() {
        let json = r#"{
            "task_id": "550e8400-e29b-41d4-a716-446655440000",
            "context": "vacation in SF",
            "created_at": "2024-01-15T10:30:00Z"
        }"#;
        let mut task: Task = serde_json::from_str(json).unwrap();
        assert_eq!(task.name, "");
        assert!(task.catalog_id.is_nil());

        task.ensure_name();
        assert_eq!(task.name, "Untitled Task 550e");
        assert_eq!(task.context, "vacation in SF");
    }

    #[test]
    fn test_ensure_name_does_not_overwrite_existing() {
        let mut task = Task::new(Uuid::new_v4(), "My Task".to_string(), "context".to_string());
        task.ensure_name();
        assert_eq!(task.name, "My Task");
    }
}
