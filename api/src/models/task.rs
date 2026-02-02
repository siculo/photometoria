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
    /// Unique identifier (UUID as String)
    pub task_id: String,

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
    /// * `context` - User-provided context information for AI analysis
    ///
    /// # Example
    /// ```
    /// use photometoria_rest_api::models::Task;
    ///
    /// let task = Task::new("vacation in San Francisco".to_string());
    /// assert!(!task.task_id.is_empty());
    /// ```
    pub fn new(context: String) -> Self {
        Self {
            task_id: Uuid::new_v4().to_string(),
            context,
            created_at: Utc::now(),
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
///   "context": "vacation in San Francisco, summer 2024"
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTaskRequest {
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
///   "task_id": "task_abc",
///   "context": "vacation in San Francisco, summer 2024",
///   "created_at": "2024-01-15T10:30:00Z"
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResponse {
    /// Unique task identifier
    pub task_id: String,

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
///   "task_id": "task_abc",
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
    pub task_id: String,

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

/// Detailed information about a task including associated jobs.
///
/// Used by: `GET /api/tasks/{task_id}` (detailed view)
///
/// # Example JSON
/// ```json
/// {
///   "task_id": "task_abc",
///   "context": "vacation in SF",
///   "created_at": "2024-01-15T10:30:00Z",
///   "photo_count": 15,
///   "storage_used": 42378436
/// }
/// ```
///
/// Note: The `jobs` field is currently commented out and will be added
/// once the Job model is implemented.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDetail {
    /// Unique task identifier
    pub task_id: String,

    /// User-provided context information
    pub context: String,

    /// Task creation timestamp (ISO 8601)
    pub created_at: DateTime<Utc>,

    /// Number of photos uploaded to this task
    pub photo_count: usize,

    /// Total storage used by photos in this task in bytes
    pub storage_used: u64,
    // TODO: Uncomment and use Vec<JobSummary> when job model is implemented
    // /// List of jobs associated with this task
    // pub jobs: Vec<JobSummary>,
}

/// Request body for updating an existing task.
///
/// Used by: `PATCH /api/tasks/{task_id}`
///
/// # Example JSON
/// ```json
/// {
///   "context": "updated context information"
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateTaskRequest {
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
            context: task.context,
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
        let task = Task::new("test context".to_string());
        assert!(!task.task_id.is_empty());
        assert!(Uuid::parse_str(&task.task_id).is_ok());
    }

    #[test]
    fn test_task_new_sets_context() {
        let context = "vacation in SF".to_string();
        let task = Task::new(context.clone());
        assert_eq!(task.context, context);
    }

    #[test]
    fn test_task_to_response_conversion() {
        let task = Task::new("test".to_string());
        let response: TaskResponse = task.clone().into();

        assert_eq!(response.task_id, task.task_id);
        assert_eq!(response.context, task.context);
        assert_eq!(response.created_at, task.created_at);
    }

    #[test]
    fn test_task_serialization() {
        let task = Task::new("test context".to_string());
        let json = serde_json::to_string(&task).unwrap();

        assert!(json.contains("task_id"));
        assert!(json.contains("context"));
        assert!(json.contains("created_at"));
    }

    #[test]
    fn test_create_task_request_deserialization() {
        let json = r#"{"context":"vacation in SF"}"#;
        let request: CreateTaskRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.context, "vacation in SF");
    }
}
