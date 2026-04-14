// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 The Photometoria contributors

//! Job data model and related DTOs
//!
//! This module defines the Job entity and all related Data Transfer Objects (DTOs)
//! for the REST API endpoints.
//!
//! A Job represents an AI analysis process that runs on a set of photos within a Task.

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
/// Stored in Job.results HashMap, keyed by photo_id.
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
// Job Status Enum
// ============================================================================

/// Status of a Job in its lifecycle.
///
/// ## State Transitions
/// ```text
/// Queued → Processing → Completed
///                    ↘ Failed
///        ↘ Cancelled (from any state)
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    /// Job is waiting in queue to be processed
    Queued,
    /// Job is currently being processed by a worker
    Processing,
    /// Job completed successfully (all photos processed)
    Completed,
    /// Job failed (unrecoverable error)
    Failed,
    /// Job was cancelled by user
    Cancelled,
}

impl std::fmt::Display for JobStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JobStatus::Queued => write!(f, "queued"),
            JobStatus::Processing => write!(f, "processing"),
            JobStatus::Completed => write!(f, "completed"),
            JobStatus::Failed => write!(f, "failed"),
            JobStatus::Cancelled => write!(f, "cancelled"),
        }
    }
}

// ============================================================================
// Job Entity (Internal Domain Model)
// ============================================================================

/// Represents an AI analysis job.
///
/// A Job belongs to a Task and processes a set of photos using a specified AI model.
/// Jobs are executed by workers and emit progress events via SSE.
///
/// ## Lifecycle
/// ```text
/// Created (Queued) → Started (Processing) → Finished (Completed/Failed/Cancelled)
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    /// Unique identifier (UUID)
    pub job_id: Uuid,

    /// Reference to the parent Task
    pub task_id: Uuid,

    /// AI model to use for analysis (e.g., "qwen3-vl:8b", "llava")
    pub model: String,

    /// Language for AI-generated tags (e.g., "English", "Italian").
    /// None means use the server-side default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,

    /// Current status of the job
    pub status: JobStatus,

    /// IDs of photos to process
    pub photo_ids: Vec<Uuid>,

    /// IDs of photos to still not processed
    pub queued_photo_ids: Vec<Uuid>,

    /// IDs of photos that has been processed
    pub processed_photo_ids: Vec<Uuid>,

    /// Timestamp when the job was created (ISO 8601)
    pub created_at: DateTime<Utc>,

    /// Timestamp when processing started (None if still queued)
    pub started_at: Option<DateTime<Utc>>,

    /// Timestamp when processing completed (None if not finished)
    pub completed_at: Option<DateTime<Utc>>,

    /// AI analysis results for each photo (photo_id -> result)
    #[serde(default)]
    pub results: HashMap<Uuid, PhotoResult>,
}

impl Job {
    /// Creates a new Job with generated UUID and current timestamp.
    ///
    /// # Arguments
    /// * `task_id` - The UUID of the parent Task
    /// * `model` - The AI model to use for analysis
    /// * `language` - Optional language for generated tags (None = server default)
    /// * `photo_ids` - UUIDs of photos to process
    ///
    /// # Example
    /// ```
    /// use photometoria_rest_api::models::Job;
    /// use uuid::Uuid;
    ///
    /// let job = Job::new(
    ///     Uuid::new_v4(),
    ///     "qwen3-vl:8b".to_string(),
    ///     None,
    ///     vec![Uuid::new_v4(), Uuid::new_v4()],
    /// );
    /// assert_eq!(job.status.to_string(), "queued");
    /// ```
    pub fn new(
        task_id: Uuid,
        model: String,
        language: Option<String>,
        photo_ids: Vec<Uuid>,
    ) -> Self {
        let not_processed_photo_ids = photo_ids.clone();
        Self {
            job_id: Uuid::new_v4(),
            task_id,
            model,
            language,
            status: JobStatus::Queued,
            photo_ids,
            queued_photo_ids: not_processed_photo_ids,
            processed_photo_ids: Vec::new(),
            created_at: Utc::now(),
            started_at: None,
            completed_at: None,
            results: HashMap::new(),
        }
    }

    /// Returns the number of photos in this job.
    pub fn photo_count(&self) -> usize {
        self.photo_ids.len()
    }

    /// Returns the number of photos in this job to be processed.
    pub fn queued_photo_count(&self) -> usize {
        self.queued_photo_ids.len()
    }

    /// Returns the number of processed photos in this job.
    pub fn processed_photo_count(&self) -> usize {
        self.processed_photo_ids.len()
    }

    /// Marks the job as started (Processing).
    pub fn start(&mut self) {
        self.status = JobStatus::Processing;
        self.started_at = Some(Utc::now());
    }

    /// Marks the job as completed.
    pub fn complete(&mut self) {
        self.status = JobStatus::Completed;
        self.completed_at = Some(Utc::now());
    }

    /// Marks the job as failed.
    pub fn fail(&mut self) {
        self.status = JobStatus::Failed;
        self.completed_at = Some(Utc::now());
    }

    /// Marks the job as cancelled.
    pub fn cancel(&mut self) {
        self.status = JobStatus::Cancelled;
        self.completed_at = Some(Utc::now());
    }

    /// Returns true if the job is in a terminal state.
    pub fn is_finished(&self) -> bool {
        matches!(
            self.status,
            JobStatus::Completed | JobStatus::Failed | JobStatus::Cancelled
        )
    }

    /// Calculates progress information from results (only valid when Processing).
    ///
    /// Returns None if the job is not in Processing status.
    pub fn calculate_progress(&self) -> Option<JobProgress> {
        if self.status != JobStatus::Processing {
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

        Some(JobProgress {
            completed,
            failed,
            remaining,
            current_photo_id,
        })
    }

    /// Returns a list of photo IDs that failed processing.
    ///
    /// Used by retry endpoint to create a new job with only failed photos.
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

/// Request body for creating a new job.
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
/// Note: `photo_ids: null` means "process all photos in the task"
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateJobRequest {
    /// AI model to use for analysis
    pub model: String,

    /// Language for AI-generated tags (e.g., "English", "Italian").
    /// If omitted, the server-side default is used.
    #[serde(default)]
    pub language: Option<String>,

    /// Photo IDs to process (None = all photos in task)
    pub photo_ids: Option<Vec<Uuid>>,
}

/// Full response for job creation and detail endpoints.
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
pub struct JobResponse {
    /// Unique job identifier
    pub job_id: Uuid,

    /// Parent task identifier
    pub task_id: Uuid,

    /// Current status
    pub status: JobStatus,

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

/// Summary information about a job for listing endpoints.
///
/// Used by:
/// - `GET /api/jobs` (list all jobs)
/// - `GET /api/tasks/{task_id}` (jobs array in TaskDetail)
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
pub struct JobSummary {
    /// Unique job identifier
    pub job_id: Uuid,

    /// Parent task identifier
    pub task_id: Uuid,

    /// Current status
    pub status: JobStatus,

    /// AI model used
    pub model: String,

    /// Language for AI-generated tags
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,

    /// Number of photos in this job
    pub photo_count: usize,

    /// Number of photos in this job to be processed
    pub queued_photo_count: usize,

    /// Number of processed photos in this job
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

/// Response for job cancellation.
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
pub struct JobCancelledResponse {
    /// Job identifier
    pub job_id: Uuid,

    /// Status (always "cancelled")
    pub status: JobStatus,
}

/// Progress information for a processing job.
///
/// Included in JobDetailResponse when status is Processing.
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
pub struct JobProgress {
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
/// Extends JobResponse with progress field (present only when Processing).
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
pub struct JobDetailResponse {
    /// Unique job identifier
    pub job_id: Uuid,

    /// Parent task identifier
    pub task_id: Uuid,

    /// Current status
    pub status: JobStatus,

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
    pub progress: Option<JobProgress>,
}

/// Summary of results by status.
///
/// Used in JobResultsResponse.
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
    /// Total number of photos in job
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
///     },
///     {
///       "photo_id": "550e8400-e29b-41d4-a716-446655440003",
///       "status": "failed",
///       "error": "Model timeout",
///       "processed_at": "2024-01-15T10:37:00Z"
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
pub struct JobResultsResponse {
    /// Job identifier
    pub job_id: Uuid,

    /// List of photo results
    pub results: Vec<PhotoResult>,

    /// Summary counts
    pub summary: ResultSummary,
}

/// Response for POST /api/jobs/{job_id}/retry.
///
/// Creates a new job for failed photos, linking to the original job.
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
pub struct RetryJobResponse {
    /// New job identifier
    pub job_id: Uuid,

    /// Parent task identifier
    pub task_id: Uuid,

    /// Status (always "queued")
    pub status: JobStatus,

    /// AI model (same as original job)
    pub model: String,

    /// Language for AI-generated tags (same as original job)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,

    /// Number of failed photos being retried
    pub photo_count: usize,

    /// Creation timestamp (ISO 8601)
    pub created_at: DateTime<Utc>,

    /// ID of the original job being retried
    pub parent_job_id: Uuid,
}

// ============================================================================
// Conversions from Entity to DTOs
// ============================================================================

impl From<Job> for JobResponse {
    fn from(job: Job) -> Self {
        Self {
            job_id: job.job_id,
            task_id: job.task_id,
            status: job.status,
            model: job.model,
            language: job.language,
            photo_count: job.photo_ids.len(),
            created_at: job.created_at,
            started_at: job.started_at,
            completed_at: job.completed_at,
        }
    }
}

impl From<&Job> for JobResponse {
    fn from(job: &Job) -> Self {
        Self {
            job_id: job.job_id,
            task_id: job.task_id,
            status: job.status,
            model: job.model.clone(),
            language: job.language.clone(),
            photo_count: job.photo_ids.len(),
            created_at: job.created_at,
            started_at: job.started_at,
            completed_at: job.completed_at,
        }
    }
}

impl From<Job> for JobSummary {
    fn from(job: Job) -> Self {
        Self {
            photo_count: job.photo_count(),
            queued_photo_count: job.queued_photo_count(),
            processed_photo_count: job.processed_photo_count(),
            job_id: job.job_id,
            task_id: job.task_id,
            status: job.status,
            model: job.model.clone(),
            language: job.language.clone(),
            created_at: job.created_at,
            started_at: job.started_at,
            completed_at: job.completed_at,
        }
    }
}

impl From<&Job> for JobSummary {
    fn from(job: &Job) -> Self {
        Self {
            photo_count: job.photo_count(),
            queued_photo_count: job.queued_photo_count(),
            processed_photo_count: job.processed_photo_count(),
            job_id: job.job_id,
            task_id: job.task_id,
            status: job.status,
            model: job.model.clone(),
            language: job.language.clone(),
            created_at: job.created_at,
            started_at: job.started_at,
            completed_at: job.completed_at,
        }
    }
}

impl From<Job> for JobDetailResponse {
    fn from(job: Job) -> Self {
        let progress = job.calculate_progress();
        Self {
            job_id: job.job_id,
            task_id: job.task_id,
            status: job.status,
            model: job.model,
            language: job.language,
            photo_count: job.photo_ids.len(),
            created_at: job.created_at,
            started_at: job.started_at,
            completed_at: job.completed_at,
            progress,
        }
    }
}

impl From<&Job> for JobDetailResponse {
    fn from(job: &Job) -> Self {
        let progress = job.calculate_progress();
        Self {
            job_id: job.job_id,
            task_id: job.task_id,
            status: job.status,
            model: job.model.clone(),
            language: job.language.clone(),
            photo_count: job.photo_ids.len(),
            created_at: job.created_at,
            started_at: job.started_at,
            completed_at: job.completed_at,
            progress,
        }
    }
}

impl From<Job> for JobResultsResponse {
    fn from(job: Job) -> Self {
        let results: Vec<PhotoResult> = job.results.values().cloned().collect();
        let completed = results
            .iter()
            .filter(|r| r.status == PhotoResultStatus::Completed)
            .count();
        let failed = results
            .iter()
            .filter(|r| r.status == PhotoResultStatus::Failed)
            .count();

        Self {
            job_id: job.job_id,
            results,
            summary: ResultSummary {
                total: job.photo_ids.len(),
                completed,
                failed,
            },
        }
    }
}

impl From<&Job> for JobResultsResponse {
    fn from(job: &Job) -> Self {
        let results: Vec<PhotoResult> = job.results.values().cloned().collect();
        let completed = results
            .iter()
            .filter(|r| r.status == PhotoResultStatus::Completed)
            .count();
        let failed = results
            .iter()
            .filter(|r| r.status == PhotoResultStatus::Failed)
            .count();

        Self {
            job_id: job.job_id,
            results,
            summary: ResultSummary {
                total: job.photo_ids.len(),
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
    fn test_job_new_generates_uuid() {
        let task_id = Uuid::new_v4();
        let photo_id = Uuid::new_v4();
        let job = Job::new(task_id, "qwen3-vl:8b".to_string(), None, vec![photo_id]);
        assert!(!job.job_id.is_nil());
    }

    #[test]
    fn test_job_new_sets_fields() {
        let task_id = Uuid::new_v4();
        let photo_ids = vec![Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()];
        let job = Job::new(task_id, "llava".to_string(), None, photo_ids.clone());

        assert_eq!(job.task_id, task_id);
        assert_eq!(job.model, "llava");
        assert_eq!(job.status, JobStatus::Queued);
        assert_eq!(job.photo_ids, photo_ids);
        assert_eq!(job.queued_photo_ids, photo_ids);
        assert_eq!(job.processed_photo_ids, Vec::<Uuid>::new());
        assert!(job.started_at.is_none());
        assert!(job.completed_at.is_none());
    }

    #[test]
    fn test_job_photo_count() {
        let task_id = Uuid::new_v4();
        let job = Job::new(
            task_id,
            "qwen3-vl:8b".to_string(),
            None,
            vec![Uuid::new_v4(), Uuid::new_v4()],
        );
        assert_eq!(job.photo_count(), 2);
        assert_eq!(job.queued_photo_count(), 2);
        assert_eq!(job.processed_photo_count(), 0);
    }

    #[test]
    fn test_job_lifecycle_start() {
        let task_id = Uuid::new_v4();
        let mut job = Job::new(
            task_id,
            "qwen3-vl:8b".to_string(),
            None,
            vec![Uuid::new_v4()],
        );

        assert_eq!(job.status, JobStatus::Queued);
        assert!(job.started_at.is_none());

        job.start();

        assert_eq!(job.status, JobStatus::Processing);
        assert!(job.started_at.is_some());
    }

    #[test]
    fn test_job_lifecycle_complete() {
        let task_id = Uuid::new_v4();
        let mut job = Job::new(
            task_id,
            "qwen3-vl:8b".to_string(),
            None,
            vec![Uuid::new_v4()],
        );

        job.start();
        job.complete();

        assert_eq!(job.status, JobStatus::Completed);
        assert!(job.completed_at.is_some());
        assert!(job.is_finished());
    }

    #[test]
    fn test_job_lifecycle_fail() {
        let task_id = Uuid::new_v4();
        let mut job = Job::new(
            task_id,
            "qwen3-vl:8b".to_string(),
            None,
            vec![Uuid::new_v4()],
        );

        job.start();
        job.fail();

        assert_eq!(job.status, JobStatus::Failed);
        assert!(job.is_finished());
    }

    #[test]
    fn test_job_lifecycle_cancel() {
        let task_id = Uuid::new_v4();
        let mut job = Job::new(
            task_id,
            "qwen3-vl:8b".to_string(),
            None,
            vec![Uuid::new_v4()],
        );

        job.cancel();

        assert_eq!(job.status, JobStatus::Cancelled);
        assert!(job.is_finished());
    }

    #[test]
    fn test_job_is_finished() {
        let task_id = Uuid::new_v4();
        let mut job = Job::new(
            task_id,
            "qwen3-vl:8b".to_string(),
            None,
            vec![Uuid::new_v4()],
        );

        assert!(!job.is_finished()); // Queued
        job.start();
        assert!(!job.is_finished()); // Processing
        job.complete();
        assert!(job.is_finished()); // Completed
    }

    #[test]
    fn test_job_status_serialization() {
        assert_eq!(
            serde_json::to_string(&JobStatus::Queued).unwrap(),
            "\"queued\""
        );
        assert_eq!(
            serde_json::to_string(&JobStatus::Processing).unwrap(),
            "\"processing\""
        );
        assert_eq!(
            serde_json::to_string(&JobStatus::Completed).unwrap(),
            "\"completed\""
        );
        assert_eq!(
            serde_json::to_string(&JobStatus::Failed).unwrap(),
            "\"failed\""
        );
        assert_eq!(
            serde_json::to_string(&JobStatus::Cancelled).unwrap(),
            "\"cancelled\""
        );
    }

    #[test]
    fn test_job_status_deserialization() {
        assert_eq!(
            serde_json::from_str::<JobStatus>("\"queued\"").unwrap(),
            JobStatus::Queued
        );
        assert_eq!(
            serde_json::from_str::<JobStatus>("\"processing\"").unwrap(),
            JobStatus::Processing
        );
    }

    #[test]
    fn test_job_to_response_conversion() {
        let task_id = Uuid::new_v4();
        let mut job = Job::new(
            task_id,
            "qwen3-vl:8b".to_string(),
            None,
            vec![Uuid::new_v4(), Uuid::new_v4()],
        );
        job.start();

        let response: JobResponse = (&job).into();

        assert_eq!(response.job_id, job.job_id);
        assert_eq!(response.task_id, job.task_id);
        assert_eq!(response.status, JobStatus::Processing);
        assert_eq!(response.model, "qwen3-vl:8b");
        assert_eq!(response.photo_count, 2);
        assert!(response.started_at.is_some());
        assert!(response.completed_at.is_none());
    }

    #[test]
    fn test_job_to_summary_conversion() {
        let task_id = Uuid::new_v4();
        let job = Job::new(task_id, "llava".to_string(), None, vec![Uuid::new_v4()]);

        let summary: JobSummary = (&job).into();

        assert_eq!(summary.job_id, job.job_id);
        assert_eq!(summary.task_id, task_id);
        assert_eq!(summary.status, JobStatus::Queued);
        assert_eq!(summary.model, "llava");
        assert_eq!(summary.photo_count, 1);
        assert_eq!(summary.queued_photo_count, 1);
        assert_eq!(summary.processed_photo_count, 0);
    }

    #[test]
    fn test_create_job_request_deserialization() {
        let json = r#"{"model":"qwen3-vl:8b","photo_ids":null}"#;
        let request: CreateJobRequest = serde_json::from_str(json).unwrap();

        assert_eq!(request.model, "qwen3-vl:8b");
        assert!(request.photo_ids.is_none());
    }

    #[test]
    fn test_create_job_request_with_photo_ids() {
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        let json = format!(r#"{{"model":"llava","photo_ids":["{}","{}"]}}"#, id1, id2);
        let request: CreateJobRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(request.model, "llava");
        assert_eq!(request.photo_ids, Some(vec![id1, id2]));
    }

    #[test]
    fn test_job_response_skips_none_timestamps() {
        let task_id = Uuid::new_v4();
        let job = Job::new(
            task_id,
            "qwen3-vl:8b".to_string(),
            None,
            vec![Uuid::new_v4()],
        );
        let response: JobResponse = job.into();
        let json = serde_json::to_string(&response).unwrap();

        // started_at and completed_at should not appear when None
        assert!(!json.contains("started_at"));
        assert!(!json.contains("completed_at"));
    }

    #[test]
    fn test_job_serialization() {
        let task_id = Uuid::new_v4();
        let job = Job::new(
            task_id,
            "qwen3-vl:8b".to_string(),
            None,
            vec![Uuid::new_v4()],
        );
        let json = serde_json::to_string(&job).unwrap();

        assert!(json.contains("job_id"));
        assert!(json.contains("task_id"));
        assert!(json.contains("model"));
        assert!(json.contains("status"));
        assert!(json.contains("photo_ids"));
        assert!(json.contains("queued_photo_ids"));
        assert!(json.contains("processed_photo_ids"));
        assert!(json.contains("created_at"));
    }

    // ========================================================================
    // Tests for PhotoResult and Progress
    // ========================================================================

    #[test]
    fn test_job_new_initializes_empty_results() {
        let task_id = Uuid::new_v4();
        let job = Job::new(
            task_id,
            "qwen3-vl:8b".to_string(),
            None,
            vec![Uuid::new_v4()],
        );
        assert!(job.results.is_empty());
    }

    #[test]
    fn test_calculate_progress_returns_none_when_not_processing() {
        let task_id = Uuid::new_v4();
        let job = Job::new(
            task_id,
            "qwen3-vl:8b".to_string(),
            None,
            vec![Uuid::new_v4(), Uuid::new_v4()],
        );

        // Should be None when Queued
        assert!(job.calculate_progress().is_none());

        let mut completed_job = job.clone();
        completed_job.complete();
        // Should be None when Completed
        assert!(completed_job.calculate_progress().is_none());
    }

    #[test]
    fn test_calculate_progress_when_processing() {
        let task_id = Uuid::new_v4();
        let photo1 = Uuid::new_v4();
        let photo2 = Uuid::new_v4();
        let photo3 = Uuid::new_v4();

        let mut job = Job::new(
            task_id,
            "qwen3-vl:8b".to_string(),
            None,
            vec![photo1, photo2, photo3],
        );
        job.start();

        // Add some results
        job.results.insert(
            photo1,
            PhotoResult {
                photo_id: photo1,
                client_id: None,
                status: PhotoResultStatus::Completed,
                tags: Some("test".to_string()),
                error: None,
                processed_at: Some(Utc::now()),
            },
        );

        job.results.insert(
            photo2,
            PhotoResult {
                photo_id: photo2,
                client_id: None,
                status: PhotoResultStatus::Failed,
                tags: None,
                error: Some("error".to_string()),
                processed_at: Some(Utc::now()),
            },
        );

        let progress = job.calculate_progress().unwrap();
        assert_eq!(progress.completed, 1);
        assert_eq!(progress.failed, 1);
        assert_eq!(progress.remaining, 1);
    }

    #[test]
    fn test_calculate_progress_current_photo_id() {
        let task_id = Uuid::new_v4();
        let photo1 = Uuid::new_v4();
        let photo2 = Uuid::new_v4();

        let mut job = Job::new(
            task_id,
            "qwen3-vl:8b".to_string(),
            None,
            vec![photo1, photo2],
        );
        job.start();

        let progress = job.calculate_progress().unwrap();
        // First photo in queued_photo_ids should be current
        assert_eq!(progress.current_photo_id, Some(photo1));
    }

    #[test]
    fn test_failed_photo_ids_empty() {
        let task_id = Uuid::new_v4();
        let job = Job::new(
            task_id,
            "qwen3-vl:8b".to_string(),
            None,
            vec![Uuid::new_v4(), Uuid::new_v4()],
        );

        assert!(job.failed_photo_ids().is_empty());
    }

    #[test]
    fn test_failed_photo_ids_returns_only_failed() {
        let task_id = Uuid::new_v4();
        let photo1 = Uuid::new_v4();
        let photo2 = Uuid::new_v4();
        let photo3 = Uuid::new_v4();

        let mut job = Job::new(
            task_id,
            "qwen3-vl:8b".to_string(),
            None,
            vec![photo1, photo2, photo3],
        );

        // Add mixed results
        job.results.insert(
            photo1,
            PhotoResult {
                photo_id: photo1,
                client_id: None,
                status: PhotoResultStatus::Completed,
                tags: Some("test".to_string()),
                error: None,
                processed_at: Some(Utc::now()),
            },
        );

        job.results.insert(
            photo2,
            PhotoResult {
                photo_id: photo2,
                client_id: None,
                status: PhotoResultStatus::Failed,
                tags: None,
                error: Some("error".to_string()),
                processed_at: Some(Utc::now()),
            },
        );

        job.results.insert(
            photo3,
            PhotoResult {
                photo_id: photo3,
                client_id: None,
                status: PhotoResultStatus::Failed,
                tags: None,
                error: Some("another error".to_string()),
                processed_at: Some(Utc::now()),
            },
        );

        let failed_ids = job.failed_photo_ids();
        assert_eq!(failed_ids.len(), 2);
        assert!(failed_ids.contains(&photo2));
        assert!(failed_ids.contains(&photo3));
        assert!(!failed_ids.contains(&photo1));
    }

    #[test]
    fn test_photo_result_status_serialization() {
        assert_eq!(
            serde_json::to_string(&PhotoResultStatus::Completed).unwrap(),
            "\"completed\""
        );
        assert_eq!(
            serde_json::to_string(&PhotoResultStatus::Failed).unwrap(),
            "\"failed\""
        );
    }

    #[test]
    fn test_job_to_detail_response_conversion() {
        let task_id = Uuid::new_v4();
        let mut job = Job::new(
            task_id,
            "qwen3-vl:8b".to_string(),
            None,
            vec![Uuid::new_v4(), Uuid::new_v4()],
        );
        job.start();

        let response: JobDetailResponse = (&job).into();

        assert_eq!(response.job_id, job.job_id);
        assert_eq!(response.task_id, job.task_id);
        assert_eq!(response.status, JobStatus::Processing);
        assert_eq!(response.model, "qwen3-vl:8b");
        assert_eq!(response.photo_count, 2);
        assert!(response.started_at.is_some());
        assert!(response.progress.is_some());
    }

    #[test]
    fn test_job_to_detail_response_no_progress_when_queued() {
        let task_id = Uuid::new_v4();
        let job = Job::new(
            task_id,
            "qwen3-vl:8b".to_string(),
            None,
            vec![Uuid::new_v4(), Uuid::new_v4()],
        );

        let response: JobDetailResponse = job.into();

        assert_eq!(response.status, JobStatus::Queued);
        assert!(response.progress.is_none());
    }

    #[test]
    fn test_job_to_results_response_conversion() {
        let task_id = Uuid::new_v4();
        let photo1 = Uuid::new_v4();
        let photo2 = Uuid::new_v4();

        let mut job = Job::new(
            task_id,
            "qwen3-vl:8b".to_string(),
            None,
            vec![photo1, photo2],
        );

        job.results.insert(
            photo1,
            PhotoResult {
                photo_id: photo1,
                client_id: None,
                status: PhotoResultStatus::Completed,
                tags: Some("test".to_string()),
                error: None,
                processed_at: Some(Utc::now()),
            },
        );

        let response: JobResultsResponse = job.into();

        assert_eq!(response.results.len(), 1);
        assert_eq!(response.summary.total, 2);
        assert_eq!(response.summary.completed, 1);
        assert_eq!(response.summary.failed, 0);
    }

    #[test]
    fn test_job_to_results_response_with_failures() {
        let task_id = Uuid::new_v4();
        let photo1 = Uuid::new_v4();
        let photo2 = Uuid::new_v4();
        let photo3 = Uuid::new_v4();

        let mut job = Job::new(
            task_id,
            "qwen3-vl:8b".to_string(),
            None,
            vec![photo1, photo2, photo3],
        );

        job.results.insert(
            photo1,
            PhotoResult {
                photo_id: photo1,
                client_id: None,
                status: PhotoResultStatus::Completed,
                tags: Some("test".to_string()),
                error: None,
                processed_at: Some(Utc::now()),
            },
        );

        job.results.insert(
            photo2,
            PhotoResult {
                photo_id: photo2,
                client_id: None,
                status: PhotoResultStatus::Failed,
                tags: None,
                error: Some("error".to_string()),
                processed_at: Some(Utc::now()),
            },
        );

        let response: JobResultsResponse = job.into();

        assert_eq!(response.results.len(), 2);
        assert_eq!(response.summary.total, 3);
        assert_eq!(response.summary.completed, 1);
        assert_eq!(response.summary.failed, 1);
    }

    #[test]
    fn test_photo_result_skips_none_fields() {
        let result = PhotoResult {
            photo_id: Uuid::new_v4(),
            client_id: None,
            status: PhotoResultStatus::Failed,
            tags: None,
            error: Some("error".to_string()),
            processed_at: None,
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(!json.contains("\"tags\""));
        assert!(json.contains("\"error\""));
        assert!(!json.contains("\"processed_at\""));
    }

    // ========================================================================
    // Language field tests
    // ========================================================================

    #[test]
    fn test_job_new_with_language() {
        let task_id = Uuid::new_v4();
        let job = Job::new(
            task_id,
            "qwen3-vl:8b".to_string(),
            Some("Italian".to_string()),
            vec![Uuid::new_v4()],
        );
        assert_eq!(job.language.as_deref(), Some("Italian"));
    }

    #[test]
    fn test_job_new_without_language() {
        let task_id = Uuid::new_v4();
        let job = Job::new(task_id, "llava".to_string(), None, vec![Uuid::new_v4()]);
        assert!(job.language.is_none());
    }

    #[test]
    fn test_job_response_includes_language() {
        let task_id = Uuid::new_v4();
        let job = Job::new(
            task_id,
            "qwen3-vl:8b".to_string(),
            Some("French".to_string()),
            vec![Uuid::new_v4()],
        );

        let response: JobResponse = (&job).into();
        assert_eq!(response.language.as_deref(), Some("French"));
    }

    #[test]
    fn test_job_response_skips_none_language() {
        let task_id = Uuid::new_v4();
        let job = Job::new(task_id, "llava".to_string(), None, vec![Uuid::new_v4()]);

        let response: JobResponse = job.into();
        let json = serde_json::to_string(&response).unwrap();
        assert!(!json.contains("\"language\""));
    }

    #[test]
    fn test_job_summary_includes_language() {
        let task_id = Uuid::new_v4();
        let job = Job::new(
            task_id,
            "llava".to_string(),
            Some("Spanish".to_string()),
            vec![Uuid::new_v4()],
        );

        let summary: JobSummary = (&job).into();
        assert_eq!(summary.language.as_deref(), Some("Spanish"));
    }

    #[test]
    fn test_job_detail_response_includes_language() {
        let task_id = Uuid::new_v4();
        let mut job = Job::new(
            task_id,
            "qwen3-vl:8b".to_string(),
            Some("German".to_string()),
            vec![Uuid::new_v4()],
        );
        job.start();

        let response: JobDetailResponse = (&job).into();
        assert_eq!(response.language.as_deref(), Some("German"));
    }

    #[test]
    fn test_create_job_request_with_language() {
        let json = r#"{"model":"qwen3-vl:8b","language":"Italian","photo_ids":null}"#;
        let request: CreateJobRequest = serde_json::from_str(json).unwrap();

        assert_eq!(request.model, "qwen3-vl:8b");
        assert_eq!(request.language.as_deref(), Some("Italian"));
        assert!(request.photo_ids.is_none());
    }

    #[test]
    fn test_create_job_request_without_language() {
        let json = r#"{"model":"llava","photo_ids":null}"#;
        let request: CreateJobRequest = serde_json::from_str(json).unwrap();

        assert!(request.language.is_none());
    }

    #[test]
    fn test_job_serialization_includes_language() {
        let task_id = Uuid::new_v4();
        let job = Job::new(
            task_id,
            "qwen3-vl:8b".to_string(),
            Some("Italian".to_string()),
            vec![Uuid::new_v4()],
        );
        let json = serde_json::to_string(&job).unwrap();
        assert!(json.contains("\"language\":\"Italian\""));
    }

    #[test]
    fn test_job_serialization_skips_none_language() {
        let task_id = Uuid::new_v4();
        let job = Job::new(task_id, "llava".to_string(), None, vec![Uuid::new_v4()]);
        let json = serde_json::to_string(&job).unwrap();
        assert!(!json.contains("\"language\""));
    }

    #[test]
    fn test_job_deserialization_with_language() {
        let task_id = Uuid::new_v4();
        let job = Job::new(
            task_id,
            "qwen3-vl:8b".to_string(),
            Some("Italian".to_string()),
            vec![Uuid::new_v4()],
        );
        let json = serde_json::to_string(&job).unwrap();
        let deserialized: Job = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.language.as_deref(), Some("Italian"));
    }

    #[test]
    fn test_job_deserialization_without_language_field() {
        let json = r#"{
            "job_id": "550e8400-e29b-41d4-a716-446655440000",
            "task_id": "550e8400-e29b-41d4-a716-446655440001",
            "model": "llava",
            "status": "queued",
            "photo_ids": [],
            "queued_photo_ids": [],
            "processed_photo_ids": [],
            "created_at": "2024-01-15T10:35:00Z"
        }"#;
        let job: Job = serde_json::from_str(json).unwrap();
        assert!(
            job.language.is_none(),
            "Missing language field should deserialize as None for backward compatibility"
        );
    }
}
