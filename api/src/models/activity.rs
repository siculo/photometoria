// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 The Photometoria contributors

//! Activity data model and related DTOs
//!
//! This module defines the Activity entity and all related Data Transfer Objects (DTOs)
//! for the REST API endpoints.
//!
//! An Activity represents an AI analysis process that runs on a set of photos within a Project.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

// ============================================================================
// PhotoResult and Status
// ============================================================================

/// Status of an individual photo's AI analysis result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PhotoResultStatus {
    /// Photo analysis completed successfully
    Completed,
    /// Photo analysis failed
    Failed,
}

/// Result of AI analysis for a single photo.
///
/// Stored in Activity.results HashMap, keyed by photo_id.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhotoResult {
    /// ID of the photo this result belongs to
    pub photo_id: Uuid,

    /// Client-provided identifier for reconciliation
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,

    /// Status of the analysis (completed or failed)
    pub status: PhotoResultStatus,

    /// Generated tags/keywords (None if failed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<String>,

    /// Error message (None if completed successfully)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,

    /// Timestamp when processing finished
    #[serde(skip_serializing_if = "Option::is_none")]
    pub processed_at: Option<DateTime<Utc>>,
}

// ============================================================================
// Activity Status Enum
// ============================================================================

/// Status of an Activity in its lifecycle.
///
/// ## State Transitions
/// ```text
/// Queued → Processing → Completed
///                    ↘ Failed
///        ↘ Cancelled (from any state)
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ActivityStatus {
    /// Activity is waiting in queue to be processed
    Queued,
    /// Activity is currently being processed by a worker
    Processing,
    /// Activity completed successfully (all photos processed)
    Completed,
    /// Activity failed (unrecoverable error)
    Failed,
    /// Activity was cancelled by user
    Cancelled,
}

impl std::fmt::Display for ActivityStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ActivityStatus::Queued => write!(f, "queued"),
            ActivityStatus::Processing => write!(f, "processing"),
            ActivityStatus::Completed => write!(f, "completed"),
            ActivityStatus::Failed => write!(f, "failed"),
            ActivityStatus::Cancelled => write!(f, "cancelled"),
        }
    }
}

fn default_legacy_provider() -> String {
    "ollama".to_string()
}

// ============================================================================
// Activity Entity (Internal Domain Model)
// ============================================================================

/// Represents an AI analysis activity.
///
/// An Activity belongs to a Project and processes a set of photos using a specified AI model.
/// Activities are executed by workers and emit progress events via SSE.
///
/// ## Lifecycle
/// ```text
/// Created (Queued) → Started (Processing) → Finished (Completed/Failed/Cancelled)
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Activity {
    /// Unique identifier (UUID)
    #[serde(rename = "job_id")]
    pub activity_id: Uuid,

    /// Reference to the parent Project
    #[serde(rename = "task_id")]
    pub project_id: Uuid,

    /// Name of the AI provider resolved at activity creation (e.g., "ollama").
    ///
    /// Persisted so the provider used for analysis can be audited after the fact,
    /// independently of runtime configuration changes.
    ///
    /// Defaults to "ollama" when deserializing activities persisted before this field
    /// existed — at that time ollama was the only supported provider.
    #[serde(default = "default_legacy_provider")]
    pub provider: String,

    /// AI model to use for analysis (e.g., "qwen3-vl:8b", "llava")
    pub model: String,

    /// Language for AI-generated tags (e.g., "English", "Italian").
    /// None means use the server-side default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,

    /// Current status of the activity
    pub status: ActivityStatus,

    /// IDs of photos to process
    pub photo_ids: Vec<Uuid>,

    /// IDs of photos not yet processed
    pub queued_photo_ids: Vec<Uuid>,

    /// IDs of photos that have been processed
    pub processed_photo_ids: Vec<Uuid>,

    /// Timestamp when the activity was created (ISO 8601)
    pub created_at: DateTime<Utc>,

    /// Timestamp when processing started (None if still queued)
    pub started_at: Option<DateTime<Utc>>,

    /// Timestamp when processing completed (None if not finished)
    pub completed_at: Option<DateTime<Utc>>,

    /// AI analysis results for each photo (photo_id -> result)
    #[serde(default)]
    pub results: HashMap<Uuid, PhotoResult>,
}

impl Activity {
    /// Creates a new Activity with generated UUID and current timestamp.
    ///
    /// # Arguments
    /// * `project_id` - The UUID of the parent Project
    /// * `provider` - The AI provider name resolved at creation time
    /// * `model` - The AI model to use for analysis
    /// * `language` - Optional language for generated tags (None = server default)
    /// * `photo_ids` - UUIDs of photos to process
    ///
    /// # Example
    /// ```
    /// use photometoria_rest_api::models::Activity;
    /// use uuid::Uuid;
    ///
    /// let activity = Activity::new(
    ///     Uuid::new_v4(),
    ///     "ollama".to_string(),
    ///     "qwen3-vl:8b".to_string(),
    ///     None,
    ///     vec![Uuid::new_v4(), Uuid::new_v4()],
    /// );
    /// assert_eq!(activity.status.to_string(), "queued");
    /// ```
    pub fn new(
        project_id: Uuid,
        provider: String,
        model: String,
        language: Option<String>,
        photo_ids: Vec<Uuid>,
    ) -> Self {
        let queued_photo_ids = photo_ids.clone();
        Self {
            activity_id: Uuid::new_v4(),
            project_id,
            provider,
            model,
            language,
            status: ActivityStatus::Queued,
            photo_ids,
            queued_photo_ids,
            processed_photo_ids: Vec::new(),
            created_at: Utc::now(),
            started_at: None,
            completed_at: None,
            results: HashMap::new(),
        }
    }

    /// Returns the number of photos in this activity.
    pub fn photo_count(&self) -> usize {
        self.photo_ids.len()
    }

    /// Returns the number of photos in this activity to be processed.
    pub fn queued_photo_count(&self) -> usize {
        self.queued_photo_ids.len()
    }

    /// Returns the number of processed photos in this activity.
    pub fn processed_photo_count(&self) -> usize {
        self.processed_photo_ids.len()
    }

    /// Marks the activity as started (Processing).
    pub fn start(&mut self) {
        self.status = ActivityStatus::Processing;
        self.started_at = Some(Utc::now());
    }

    /// Marks the activity as completed.
    pub fn complete(&mut self) {
        self.status = ActivityStatus::Completed;
        self.completed_at = Some(Utc::now());
    }

    /// Marks the activity as failed.
    pub fn fail(&mut self) {
        self.status = ActivityStatus::Failed;
        self.completed_at = Some(Utc::now());
    }

    /// Marks the activity as cancelled.
    pub fn cancel(&mut self) {
        self.status = ActivityStatus::Cancelled;
        self.completed_at = Some(Utc::now());
    }

    /// Returns true if the activity is in a terminal state.
    pub fn is_finished(&self) -> bool {
        matches!(
            self.status,
            ActivityStatus::Completed | ActivityStatus::Failed | ActivityStatus::Cancelled
        )
    }

    /// Calculates progress information from results (only valid when Processing).
    ///
    /// Returns None if the activity is not in Processing status.
    pub fn calculate_progress(&self) -> Option<ActivityProgress> {
        if self.status != ActivityStatus::Processing {
            return None;
        }

        let completed = self
            .results
            .values()
            .filter(|r| r.status == PhotoResultStatus::Completed)
            .count();

        let failed = self
            .results
            .values()
            .filter(|r| r.status == PhotoResultStatus::Failed)
            .count();

        let remaining = self.photo_ids.len() - (completed + failed);

        let current_photo_id = self.queued_photo_ids.first().copied();

        Some(ActivityProgress {
            completed,
            failed,
            remaining,
            current_photo_id,
        })
    }

    /// Returns a list of photo IDs that failed processing.
    ///
    /// Used by retry endpoint to create a new activity with only failed photos.
    pub fn failed_photo_ids(&self) -> Vec<Uuid> {
        self.results
            .values()
            .filter(|r| r.status == PhotoResultStatus::Failed)
            .map(|r| r.photo_id)
            .collect()
    }

    /// Returns all photo IDs that should be retried: failed + not yet processed.
    ///
    /// Used by retry endpoint after cancellation or partial completion, to
    /// collect both photos that never ran and photos that ran but failed.
    pub fn retriable_photo_ids(&self) -> Vec<Uuid> {
        let mut ids = self.queued_photo_ids.clone();
        ids.extend(self.failed_photo_ids());
        ids
    }
}

// ============================================================================
// DTOs for API Endpoints
// ============================================================================

/// Request body for creating a new activity.
///
/// Used by: `POST /api/tasks/{task_id}/jobs`
///
/// # Example JSON
/// ```json
/// {
///   "model": "qwen3-vl:8b",
///   "language": "Italian",
///   "photo_ids": null
/// }
/// ```
///
/// Note: `photo_ids: null` means "process all photos in the project"
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateActivityRequest {
    /// AI provider to use for analysis (e.g., "ollama").
    /// Must match a registered provider name.
    pub provider: String,

    /// AI model to use for analysis
    pub model: String,

    /// Language for AI-generated tags (e.g., "English", "Italian").
    /// If omitted, the server-side default is used.
    #[serde(default)]
    pub language: Option<String>,

    /// Photo IDs to process (None = all photos in project)
    pub photo_ids: Option<Vec<Uuid>>,
}

/// Full response for activity creation and detail endpoints.
///
/// Used by:
/// - `POST /api/tasks/{task_id}/jobs` (creation response)
/// - `GET /api/jobs/{job_id}` (detail)
///
/// # Example JSON
/// ```json
/// {
///   "job_id": "550e8400-e29b-41d4-a716-446655440000",
///   "task_id": "550e8400-e29b-41d4-a716-446655440001",
///   "status": "queued",
///   "model": "qwen3-vl:8b",
///   "language": "Italian",
///   "photo_count": 15,
///   "created_at": "2024-01-15T10:35:00Z",
///   "started_at": null,
///   "completed_at": null
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityResponse {
    /// Unique activity identifier
    #[serde(rename = "job_id")]
    pub activity_id: Uuid,

    /// Parent project identifier
    #[serde(rename = "task_id")]
    pub project_id: Uuid,

    /// Current status
    pub status: ActivityStatus,

    /// AI provider used
    pub provider: String,

    /// AI model used
    pub model: String,

    /// Language for AI-generated tags
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,

    /// Number of photos to process
    pub photo_count: usize,

    /// Creation timestamp (ISO 8601)
    pub created_at: DateTime<Utc>,

    /// Processing start timestamp (ISO 8601), None if not started
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<DateTime<Utc>>,

    /// Completion timestamp (ISO 8601), None if not finished
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
}

/// Summary information about an activity for listing endpoints.
///
/// Used by:
/// - `GET /api/jobs` (list all activities)
/// - `GET /api/tasks/{task_id}` (activities array in ProjectDetail)
///
/// # Example JSON
/// ```json
/// {
///   "job_id": "550e8400-e29b-41d4-a716-446655440000",
///   "task_id": "660e8400-e29b-41d4-a716-446655440000",
///   "status": "completed",
///   "model": "qwen3-vl:8b",
///   "photo_count": 15,
///   "created_at": "2024-01-15T10:35:00Z",
///   "started_at": "2024-01-15T10:36:00Z",
///   "completed_at": "2024-01-15T10:45:00Z"
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivitySummary {
    /// Unique activity identifier
    #[serde(rename = "job_id")]
    pub activity_id: Uuid,

    /// Parent project identifier
    #[serde(rename = "task_id")]
    pub project_id: Uuid,

    /// Current status
    pub status: ActivityStatus,

    /// AI provider used
    pub provider: String,

    /// AI model used
    pub model: String,

    /// Language for AI-generated tags
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,

    /// Number of photos in this activity
    pub photo_count: usize,

    /// Number of photos in this activity to be processed
    pub queued_photo_count: usize,

    /// Number of processed photos in this activity
    pub processed_photo_count: usize,

    /// Creation timestamp (ISO 8601)
    pub created_at: DateTime<Utc>,

    /// Processing start timestamp (ISO 8601), None if not yet started
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<DateTime<Utc>>,

    /// Completion timestamp (ISO 8601), None if not finished
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
}

/// Response for activity cancellation.
///
/// Used by: `DELETE /api/jobs/{job_id}`
///
/// # Example JSON
/// ```json
/// {
///   "job_id": "550e8400-e29b-41d4-a716-446655440000",
///   "status": "cancelled"
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityCancelledResponse {
    /// Activity identifier
    #[serde(rename = "job_id")]
    pub activity_id: Uuid,

    /// Status (always "cancelled")
    pub status: ActivityStatus,
}

/// Progress information for a processing activity.
///
/// Included in ActivityDetailResponse when status is Processing.
///
/// # Example JSON
/// ```json
/// {
///   "completed": 5,
///   "failed": 1,
///   "remaining": 9,
///   "current_photo_id": "550e8400-e29b-41d4-a716-446655440003"
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityProgress {
    /// Number of photos successfully processed
    pub completed: usize,

    /// Number of photos that failed processing
    pub failed: usize,

    /// Number of photos not yet processed
    pub remaining: usize,

    /// ID of the photo currently being processed (None if between photos)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_photo_id: Option<Uuid>,
}

/// Detailed response including progress (for GET /api/jobs/{job_id}).
///
/// Extends ActivityResponse with progress field (present only when Processing).
///
/// # Example JSON
/// ```json
/// {
///   "job_id": "550e8400-e29b-41d4-a716-446655440000",
///   "task_id": "550e8400-e29b-41d4-a716-446655440001",
///   "status": "processing",
///   "model": "qwen3-vl:8b",
///   "photo_count": 15,
///   "created_at": "2024-01-15T10:35:00Z",
///   "started_at": "2024-01-15T10:35:10Z",
///   "progress": {
///     "completed": 5,
///     "failed": 1,
///     "remaining": 9,
///     "current_photo_id": "550e8400-e29b-41d4-a716-446655440003"
///   }
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityDetailResponse {
    /// Unique activity identifier
    #[serde(rename = "job_id")]
    pub activity_id: Uuid,

    /// Parent project identifier
    #[serde(rename = "task_id")]
    pub project_id: Uuid,

    /// Current status
    pub status: ActivityStatus,

    /// AI provider used
    pub provider: String,

    /// AI model used
    pub model: String,

    /// Language for AI-generated tags
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,

    /// Number of photos to process
    pub photo_count: usize,

    /// Creation timestamp (ISO 8601)
    pub created_at: DateTime<Utc>,

    /// Processing start timestamp (ISO 8601), None if not started
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<DateTime<Utc>>,

    /// Completion timestamp (ISO 8601), None if not finished
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,

    /// Progress information (only when status is Processing)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<ActivityProgress>,
}

/// Summary of results by status.
///
/// Used in ActivityResultsResponse.
///
/// # Example JSON
/// ```json
/// {
///   "total": 15,
///   "completed": 12,
///   "failed": 3
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResultSummary {
    /// Total number of photos in activity
    pub total: usize,

    /// Number of successfully completed photos
    pub completed: usize,

    /// Number of failed photos
    pub failed: usize,
}

/// Response for GET /api/jobs/{job_id}/results.
///
/// Returns AI analysis results for all processed photos.
///
/// # Example JSON
/// ```json
/// {
///   "job_id": "550e8400-e29b-41d4-a716-446655440000",
///   "results": [
///     {
///       "photo_id": "550e8400-e29b-41d4-a716-446655440002",
///       "status": "completed",
///       "tags": "landscape, mountain, sunset",
///       "processed_at": "2024-01-15T10:36:00Z"
///     }
///   ],
///   "summary": {
///     "total": 15,
///     "completed": 12,
///     "failed": 3
///   }
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityResultsResponse {
    /// Activity identifier
    #[serde(rename = "job_id")]
    pub activity_id: Uuid,

    /// List of photo results
    pub results: Vec<PhotoResult>,

    /// Summary counts
    pub summary: ResultSummary,
}

/// Response for POST /api/jobs/{job_id}/retry.
///
/// Creates a new activity for failed photos, linking to the original activity.
///
/// # Example JSON
/// ```json
/// {
///   "job_id": "550e8400-e29b-41d4-a716-446655440010",
///   "task_id": "550e8400-e29b-41d4-a716-446655440001",
///   "status": "queued",
///   "model": "qwen3-vl:8b",
///   "photo_count": 3,
///   "created_at": "2024-01-15T11:00:00Z",
///   "parent_job_id": "550e8400-e29b-41d4-a716-446655440000"
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryActivityResponse {
    /// New activity identifier
    #[serde(rename = "job_id")]
    pub activity_id: Uuid,

    /// Parent project identifier
    #[serde(rename = "task_id")]
    pub project_id: Uuid,

    /// Status (always "queued")
    pub status: ActivityStatus,

    /// AI provider (same as original activity)
    pub provider: String,

    /// AI model (same as original activity)
    pub model: String,

    /// Language for AI-generated tags (same as original activity)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,

    /// Number of failed photos being retried
    pub photo_count: usize,

    /// Creation timestamp (ISO 8601)
    pub created_at: DateTime<Utc>,

    /// ID of the original activity being retried
    #[serde(rename = "parent_job_id")]
    pub parent_activity_id: Uuid,
}

// ============================================================================
// Conversions from Entity to DTOs
// ============================================================================

impl From<Activity> for ActivityResponse {
    fn from(activity: Activity) -> Self {
        Self {
            activity_id: activity.activity_id,
            project_id: activity.project_id,
            status: activity.status,
            provider: activity.provider,
            model: activity.model,
            language: activity.language,
            photo_count: activity.photo_ids.len(),
            created_at: activity.created_at,
            started_at: activity.started_at,
            completed_at: activity.completed_at,
        }
    }
}

impl From<&Activity> for ActivityResponse {
    fn from(activity: &Activity) -> Self {
        Self {
            activity_id: activity.activity_id,
            project_id: activity.project_id,
            status: activity.status,
            provider: activity.provider.clone(),
            model: activity.model.clone(),
            language: activity.language.clone(),
            photo_count: activity.photo_ids.len(),
            created_at: activity.created_at,
            started_at: activity.started_at,
            completed_at: activity.completed_at,
        }
    }
}

impl From<Activity> for ActivitySummary {
    fn from(activity: Activity) -> Self {
        Self {
            photo_count: activity.photo_count(),
            queued_photo_count: activity.queued_photo_count(),
            processed_photo_count: activity.processed_photo_count(),
            activity_id: activity.activity_id,
            project_id: activity.project_id,
            status: activity.status,
            provider: activity.provider.clone(),
            model: activity.model.clone(),
            language: activity.language.clone(),
            created_at: activity.created_at,
            started_at: activity.started_at,
            completed_at: activity.completed_at,
        }
    }
}

impl From<&Activity> for ActivitySummary {
    fn from(activity: &Activity) -> Self {
        Self {
            photo_count: activity.photo_count(),
            queued_photo_count: activity.queued_photo_count(),
            processed_photo_count: activity.processed_photo_count(),
            activity_id: activity.activity_id,
            project_id: activity.project_id,
            status: activity.status,
            provider: activity.provider.clone(),
            model: activity.model.clone(),
            language: activity.language.clone(),
            created_at: activity.created_at,
            started_at: activity.started_at,
            completed_at: activity.completed_at,
        }
    }
}

impl From<Activity> for ActivityDetailResponse {
    fn from(activity: Activity) -> Self {
        let progress = activity.calculate_progress();
        Self {
            activity_id: activity.activity_id,
            project_id: activity.project_id,
            status: activity.status,
            provider: activity.provider,
            model: activity.model,
            language: activity.language,
            photo_count: activity.photo_ids.len(),
            created_at: activity.created_at,
            started_at: activity.started_at,
            completed_at: activity.completed_at,
            progress,
        }
    }
}

impl From<&Activity> for ActivityDetailResponse {
    fn from(activity: &Activity) -> Self {
        let progress = activity.calculate_progress();
        Self {
            activity_id: activity.activity_id,
            project_id: activity.project_id,
            status: activity.status,
            provider: activity.provider.clone(),
            model: activity.model.clone(),
            language: activity.language.clone(),
            photo_count: activity.photo_ids.len(),
            created_at: activity.created_at,
            started_at: activity.started_at,
            completed_at: activity.completed_at,
            progress,
        }
    }
}

impl From<Activity> for ActivityResultsResponse {
    fn from(activity: Activity) -> Self {
        let results: Vec<PhotoResult> = activity.results.values().cloned().collect();
        let completed = results
            .iter()
            .filter(|r| r.status == PhotoResultStatus::Completed)
            .count();
        let failed = results
            .iter()
            .filter(|r| r.status == PhotoResultStatus::Failed)
            .count();

        Self {
            activity_id: activity.activity_id,
            results,
            summary: ResultSummary {
                total: activity.photo_ids.len(),
                completed,
                failed,
            },
        }
    }
}

impl From<&Activity> for ActivityResultsResponse {
    fn from(activity: &Activity) -> Self {
        let results: Vec<PhotoResult> = activity.results.values().cloned().collect();
        let completed = results
            .iter()
            .filter(|r| r.status == PhotoResultStatus::Completed)
            .count();
        let failed = results
            .iter()
            .filter(|r| r.status == PhotoResultStatus::Failed)
            .count();

        Self {
            activity_id: activity.activity_id,
            results,
            summary: ResultSummary {
                total: activity.photo_ids.len(),
                completed,
                failed,
            },
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_activity_new_generates_uuid() {
        let project_id = Uuid::new_v4();
        let photo_id = Uuid::new_v4();
        let activity = Activity::new(
            project_id,
            "ollama".to_string(),
            "qwen3-vl:8b".to_string(),
            None,
            vec![photo_id],
        );
        assert!(!activity.activity_id.is_nil());
    }

    #[test]
    fn test_activity_new_sets_fields() {
        let project_id = Uuid::new_v4();
        let photo_ids = vec![Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()];
        let activity = Activity::new(
            project_id,
            "ollama".to_string(),
            "llava".to_string(),
            None,
            photo_ids.clone(),
        );

        assert_eq!(activity.project_id, project_id);
        assert_eq!(activity.provider, "ollama");
        assert_eq!(activity.model, "llava");
        assert_eq!(activity.status, ActivityStatus::Queued);
        assert_eq!(activity.photo_ids, photo_ids);
        assert_eq!(activity.queued_photo_ids, photo_ids);
        assert_eq!(activity.processed_photo_ids, Vec::<Uuid>::new());
        assert!(activity.started_at.is_none());
        assert!(activity.completed_at.is_none());
    }

    #[test]
    fn test_activity_photo_count() {
        let project_id = Uuid::new_v4();
        let activity = Activity::new(
            project_id,
            "ollama".to_string(),
            "qwen3-vl:8b".to_string(),
            None,
            vec![Uuid::new_v4(), Uuid::new_v4()],
        );
        assert_eq!(activity.photo_count(), 2);
        assert_eq!(activity.queued_photo_count(), 2);
        assert_eq!(activity.processed_photo_count(), 0);
    }

    #[test]
    fn test_activity_lifecycle_start() {
        let project_id = Uuid::new_v4();
        let mut activity = Activity::new(
            project_id,
            "ollama".to_string(),
            "qwen3-vl:8b".to_string(),
            None,
            vec![Uuid::new_v4()],
        );

        assert_eq!(activity.status, ActivityStatus::Queued);
        assert!(activity.started_at.is_none());

        activity.start();

        assert_eq!(activity.status, ActivityStatus::Processing);
        assert!(activity.started_at.is_some());
    }

    #[test]
    fn test_activity_lifecycle_complete() {
        let project_id = Uuid::new_v4();
        let mut activity = Activity::new(
            project_id,
            "ollama".to_string(),
            "qwen3-vl:8b".to_string(),
            None,
            vec![Uuid::new_v4()],
        );

        activity.start();
        activity.complete();

        assert_eq!(activity.status, ActivityStatus::Completed);
        assert!(activity.completed_at.is_some());
        assert!(activity.is_finished());
    }

    #[test]
    fn test_activity_lifecycle_cancel() {
        let project_id = Uuid::new_v4();
        let mut activity = Activity::new(
            project_id,
            "ollama".to_string(),
            "qwen3-vl:8b".to_string(),
            None,
            vec![Uuid::new_v4()],
        );

        activity.cancel();

        assert_eq!(activity.status, ActivityStatus::Cancelled);
        assert!(activity.completed_at.is_some());
        assert!(activity.is_finished());
    }

    #[test]
    fn test_activity_is_finished() {
        let project_id = Uuid::new_v4();

        let mut queued = Activity::new(
            project_id,
            "ollama".to_string(),
            "llava".to_string(),
            None,
            vec![],
        );
        assert!(!queued.is_finished());

        queued.start();
        assert!(!queued.is_finished());

        queued.complete();
        assert!(queued.is_finished());
    }

    #[test]
    fn test_wire_format_serialization() {
        let project_id = Uuid::new_v4();
        let activity = Activity::new(
            project_id,
            "ollama".to_string(),
            "llava".to_string(),
            None,
            vec![],
        );

        let json = serde_json::to_value(&activity).unwrap();
        assert!(json.get("job_id").is_some(), "wire field must be job_id");
        assert!(json.get("task_id").is_some(), "wire field must be task_id");
        assert!(
            json.get("activity_id").is_none(),
            "no internal field exposed"
        );
        assert!(
            json.get("project_id").is_none(),
            "no internal field exposed"
        );
    }

    #[test]
    fn test_wire_format_deserialization() {
        let activity_id = Uuid::new_v4();
        let project_id = Uuid::new_v4();
        let json = serde_json::json!({
            "job_id": activity_id,
            "task_id": project_id,
            "provider": "ollama",
            "model": "llava",
            "status": "queued",
            "photo_ids": [],
            "queued_photo_ids": [],
            "processed_photo_ids": [],
            "created_at": "2024-01-15T10:35:00Z",
        });

        let activity: Activity = serde_json::from_value(json).unwrap();
        assert_eq!(activity.activity_id, activity_id);
        assert_eq!(activity.project_id, project_id);
    }
}
