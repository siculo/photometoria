// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 The Photometoria contributors

//! Project data model and related DTOs
//!
//! This module defines the Project entity and all related Data Transfer Objects (DTOs)
//! for the REST API endpoints.
//!
//! A Project represents a working session for a photographer, containing uploaded photos
//! and shared context hints for AI analysis.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ============================================================================
// Project Entity (Internal Domain Model)
// ============================================================================

/// Represents a working session for a photographer.
///
/// A Project is a container for uploaded photos and shared context hints.
/// It's short-lived (one working session) but has no automatic timeout initially.
///
/// ## Lifecycle
/// ```text
/// Created → Photos Uploaded → Jobs Created/Executed → Explicitly Deleted
/// ```
///
/// ## Current Limitation
/// Only one active project allowed at a time (returns error if another exists).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    /// Unique identifier (UUID)
    #[serde(rename = "task_id")]
    pub project_id: Uuid,

    /// Identifier of the Lightroom catalog this project belongs to
    ///
    /// Defaults to nil UUID for backward compatibility with existing
    /// storage that predates this field.
    #[serde(default)]
    pub catalog_id: Uuid,

    /// Short human-readable name for the project (must be unique)
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

    /// Timestamp when the project was created (ISO 8601)
    pub created_at: DateTime<Utc>,
}

impl Project {
    /// Creates a new Project with generated UUID and current timestamp.
    ///
    /// # Arguments
    /// * `catalog_id` - UUID of the Lightroom catalog this project belongs to
    /// * `name` - Short human-readable name for the project
    /// * `context` - User-provided context information for AI analysis
    ///
    /// # Example
    /// ```
    /// use photometoria_rest_api::models::Project;
    /// use uuid::Uuid;
    ///
    /// let catalog_id = Uuid::new_v4();
    /// let project = Project::new(catalog_id, "SF Vacation".to_string(), "vacation in San Francisco".to_string());
    /// ```
    pub fn new(catalog_id: Uuid, name: String, context: String) -> Self {
        Self {
            project_id: Uuid::new_v4(),
            catalog_id,
            name,
            context,
            created_at: Utc::now(),
        }
    }

    /// Assigns a generated name if the current name is empty.
    ///
    /// Used for backward compatibility when loading projects from storage
    /// that predate the `name` field. Generates a name like "Untitled Project a1b2"
    /// using the first 4 hex characters of the project ID.
    pub fn ensure_name(&mut self) {
        if self.name.is_empty() {
            let short_id = &self.project_id.to_string()[..4];
            self.name = format!("Untitled Project {}", short_id);
        }
    }
}

// ============================================================================
// DTOs for API Endpoints
// ============================================================================

/// Request body for creating a new project.
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
pub struct CreateProjectRequest {
    /// Short human-readable name for the project (must be unique)
    pub name: String,

    /// User-provided context information for AI analysis
    pub context: String,
}

/// Response for project creation and basic project information.
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
pub struct ProjectResponse {
    /// Unique project identifier
    #[serde(rename = "task_id")]
    pub project_id: Uuid,

    /// Identifier of the Lightroom catalog this project belongs to
    pub catalog_id: Uuid,

    /// Short human-readable name for the project
    pub name: String,

    /// User-provided context information
    pub context: String,

    /// Project creation timestamp (ISO 8601)
    pub created_at: DateTime<Utc>,
}

/// Summary information about a project for listing endpoints.
///
/// Used by: `GET /api/tasks` (list all projects)
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
pub struct ProjectSummary {
    /// Unique project identifier
    #[serde(rename = "task_id")]
    pub project_id: Uuid,

    /// Identifier of the Lightroom catalog this project belongs to
    pub catalog_id: Uuid,

    /// Short human-readable name for the project
    pub name: String,

    /// User-provided context information
    pub context: String,

    /// Number of photos uploaded to this project
    pub photo_count: usize,

    /// Total storage used by photos in this project in bytes
    pub storage_used: u64,

    /// Project creation timestamp (ISO 8601)
    pub created_at: DateTime<Utc>,

    /// Number of jobs associated with this project
    pub job_count: usize,
}

/// Detailed information about a project including aggregated data.
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
pub struct ProjectDetail {
    /// Unique project identifier
    #[serde(rename = "task_id")]
    pub project_id: Uuid,

    /// Identifier of the Lightroom catalog this project belongs to
    pub catalog_id: Uuid,

    /// Short human-readable name for the project
    pub name: String,

    /// User-provided context information
    pub context: String,

    /// Project creation timestamp (ISO 8601)
    pub created_at: DateTime<Utc>,

    /// Number of photos uploaded to this project
    pub photo_count: usize,

    /// Total storage used by photos in this project in bytes
    pub storage_used: u64,

    /// Number of jobs associated with this project
    pub job_count: usize,
}

/// Request body for updating an existing project.
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
pub struct UpdateProjectRequest {
    /// Updated project name (must be unique)
    pub name: String,

    /// Updated context information
    pub context: String,
}

// ============================================================================
// Conversions from Entity to DTOs
// ============================================================================

impl From<Project> for ProjectResponse {
    /// Converts a Project entity into a ProjectResponse DTO.
    fn from(project: Project) -> Self {
        Self {
            project_id: project.project_id,
            catalog_id: project.catalog_id,
            name: project.name,
            context: project.context,
            created_at: project.created_at,
        }
    }
}

impl From<&Project> for ProjectResponse {
    /// Converts a Project reference into a ProjectResponse DTO.
    fn from(project: &Project) -> Self {
        Self {
            project_id: project.project_id,
            catalog_id: project.catalog_id,
            name: project.name.clone(),
            context: project.context.clone(),
            created_at: project.created_at,
        }
    }
}

// Note: ProjectSummary and ProjectDetail require additional data from PhotoStore
// and JobStore, so they don't have a simple From<Project> implementation.
// They will be constructed in the handler layer where all necessary data
// is available.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_project_new_generates_uuid() {
        let catalog_id = Uuid::new_v4();
        let project = Project::new(catalog_id, "Test".to_string(), "test context".to_string());
        assert!(!project.project_id.is_nil());
    }

    #[test]
    fn test_project_new_sets_fields() {
        let catalog_id = Uuid::new_v4();
        let project = Project::new(
            catalog_id,
            "SF Vacation".to_string(),
            "vacation in SF".to_string(),
        );
        assert_eq!(project.catalog_id, catalog_id);
        assert_eq!(project.name, "SF Vacation");
        assert_eq!(project.context, "vacation in SF");
    }

    #[test]
    fn test_project_to_response_conversion() {
        let catalog_id = Uuid::new_v4();
        let project = Project::new(catalog_id, "Test".to_string(), "test".to_string());
        let response: ProjectResponse = (&project).into();

        assert_eq!(response.project_id, project.project_id);
        assert_eq!(response.catalog_id, project.catalog_id);
        assert_eq!(response.name, project.name);
        assert_eq!(response.context, project.context);
        assert_eq!(response.created_at, project.created_at);
    }

    #[test]
    fn test_project_serialization() {
        let project = Project::new(
            Uuid::new_v4(),
            "Test".to_string(),
            "test context".to_string(),
        );
        let json = serde_json::to_string(&project).unwrap();

        assert!(json.contains("task_id"));
        assert!(json.contains("catalog_id"));
        assert!(json.contains("name"));
        assert!(json.contains("context"));
        assert!(json.contains("created_at"));
    }

    #[test]
    fn test_create_project_request_deserialization() {
        let json = r#"{"name":"SF Vacation","context":"vacation in SF"}"#;
        let request: CreateProjectRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.name, "SF Vacation");
        assert_eq!(request.context, "vacation in SF");
    }

    #[test]
    fn test_project_deserialization_without_name_and_catalog() {
        let json = r#"{
            "task_id": "550e8400-e29b-41d4-a716-446655440000",
            "context": "vacation in SF",
            "created_at": "2024-01-15T10:30:00Z"
        }"#;
        let mut project: Project = serde_json::from_str(json).unwrap();
        assert_eq!(project.name, "");
        assert!(project.catalog_id.is_nil());

        project.ensure_name();
        assert_eq!(project.name, "Untitled Project 550e");
        assert_eq!(project.context, "vacation in SF");
    }

    #[test]
    fn test_ensure_name_does_not_overwrite_existing() {
        let mut project = Project::new(
            Uuid::new_v4(),
            "My Project".to_string(),
            "context".to_string(),
        );
        project.ensure_name();
        assert_eq!(project.name, "My Project");
    }
}
