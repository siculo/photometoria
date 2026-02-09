//! Job data model and related DTOs
//!
//! This module defines the Job entity and all related Data Transfer Objects (DTOs)
//! for the REST API endpoints.
//!
//! A Job represents an AI analysis process that runs on a set of photos within a Task.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

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

    /// AI model to use for analysis (e.g., "qwen2-vl:8b", "llava")
    pub model: String,

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
}

impl Job {
    /// Creates a new Job with generated UUID and current timestamp.
    ///
    /// # Arguments
    /// * `task_id` - The UUID of the parent Task
    /// * `model` - The AI model to use for analysis
    /// * `photo_ids` - UUIDs of photos to process
    ///
    /// # Example
    /// ```
    /// use photometoria_rest_api::models::Job;
    /// use uuid::Uuid;
    ///
    /// let job = Job::new(
    ///     Uuid::new_v4(),
    ///     "qwen2-vl:8b".to_string(),
    ///     vec![Uuid::new_v4(), Uuid::new_v4()],
    /// );
    /// assert_eq!(job.status.to_string(), "queued");
    /// ```
    pub fn new(task_id: Uuid, model: String, photo_ids: Vec<Uuid>) -> Self {
        let not_processed_photo_ids = photo_ids.clone();
        Self {
            job_id: Uuid::new_v4(),
            task_id,
            model,
            status: JobStatus::Queued,
            photo_ids,
            queued_photo_ids: not_processed_photo_ids,
            processed_photo_ids: Vec::new(),
            created_at: Utc::now(),
            started_at: None,
            completed_at: None,
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
///   "model": "qwen2-vl:8b",
///   "photo_ids": null
/// }
/// ```
///
/// Note: `photo_ids: null` means "process all photos in the task"
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateJobRequest {
    /// AI model to use for analysis
    pub model: String,

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
///   "model": "qwen2-vl:8b",
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
///   "status": "completed",
///   "model": "qwen2-vl:8b",
///   "photo_count": 15,
///   "created_at": "2024-01-15T10:35:00Z",
///   "completed_at": "2024-01-15T10:45:00Z"
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobSummary {
    /// Unique job identifier
    pub job_id: Uuid,

    /// Current status
    pub status: JobStatus,

    /// AI model used
    pub model: String,

    /// Number of photos in this job
    pub photo_count: usize,

    /// Number of photos in this job to be processed
    pub queued_photo_count: usize,

    /// Number of processed photos in this job
    pub processed_photo_count: usize,

    /// Creation timestamp (ISO 8601)
    pub created_at: DateTime<Utc>,

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
            status: job.status,
            model: job.model.clone(),
            created_at: job.created_at,
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
            status: job.status,
            model: job.model.clone(),
            created_at: job.created_at,
            completed_at: job.completed_at,
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
        let job = Job::new(task_id, "qwen2-vl:8b".to_string(), vec![photo_id]);
        assert!(!job.job_id.is_nil());
    }

    #[test]
    fn test_job_new_sets_fields() {
        let task_id = Uuid::new_v4();
        let photo_ids = vec![Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()];
        let job = Job::new(task_id, "llava".to_string(), photo_ids.clone());

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
            "qwen2-vl:8b".to_string(),
            vec![Uuid::new_v4(), Uuid::new_v4()],
        );
        assert_eq!(job.photo_count(), 2);
        assert_eq!(job.queued_photo_count(), 2);
        assert_eq!(job.processed_photo_count(), 0);
    }

    #[test]
    fn test_job_lifecycle_start() {
        let task_id = Uuid::new_v4();
        let mut job = Job::new(task_id, "qwen2-vl:8b".to_string(), vec![Uuid::new_v4()]);

        assert_eq!(job.status, JobStatus::Queued);
        assert!(job.started_at.is_none());

        job.start();

        assert_eq!(job.status, JobStatus::Processing);
        assert!(job.started_at.is_some());
    }

    #[test]
    fn test_job_lifecycle_complete() {
        let task_id = Uuid::new_v4();
        let mut job = Job::new(task_id, "qwen2-vl:8b".to_string(), vec![Uuid::new_v4()]);

        job.start();
        job.complete();

        assert_eq!(job.status, JobStatus::Completed);
        assert!(job.completed_at.is_some());
        assert!(job.is_finished());
    }

    #[test]
    fn test_job_lifecycle_fail() {
        let task_id = Uuid::new_v4();
        let mut job = Job::new(task_id, "qwen2-vl:8b".to_string(), vec![Uuid::new_v4()]);

        job.start();
        job.fail();

        assert_eq!(job.status, JobStatus::Failed);
        assert!(job.is_finished());
    }

    #[test]
    fn test_job_lifecycle_cancel() {
        let task_id = Uuid::new_v4();
        let mut job = Job::new(task_id, "qwen2-vl:8b".to_string(), vec![Uuid::new_v4()]);

        job.cancel();

        assert_eq!(job.status, JobStatus::Cancelled);
        assert!(job.is_finished());
    }

    #[test]
    fn test_job_is_finished() {
        let task_id = Uuid::new_v4();
        let mut job = Job::new(task_id, "qwen2-vl:8b".to_string(), vec![Uuid::new_v4()]);

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
            "qwen2-vl:8b".to_string(),
            vec![Uuid::new_v4(), Uuid::new_v4()],
        );
        job.start();

        let response: JobResponse = (&job).into();

        assert_eq!(response.job_id, job.job_id);
        assert_eq!(response.task_id, job.task_id);
        assert_eq!(response.status, JobStatus::Processing);
        assert_eq!(response.model, "qwen2-vl:8b");
        assert_eq!(response.photo_count, 2);
        assert!(response.started_at.is_some());
        assert!(response.completed_at.is_none());
    }

    #[test]
    fn test_job_to_summary_conversion() {
        let task_id = Uuid::new_v4();
        let job = Job::new(task_id, "llava".to_string(), vec![Uuid::new_v4()]);

        let summary: JobSummary = (&job).into();

        assert_eq!(summary.job_id, job.job_id);
        assert_eq!(summary.status, JobStatus::Queued);
        assert_eq!(summary.model, "llava");
        assert_eq!(summary.photo_count, 1);
        assert_eq!(summary.queued_photo_count, 1);
        assert_eq!(summary.processed_photo_count, 0);
    }

    #[test]
    fn test_create_job_request_deserialization() {
        let json = r#"{"model":"qwen2-vl:8b","photo_ids":null}"#;
        let request: CreateJobRequest = serde_json::from_str(json).unwrap();

        assert_eq!(request.model, "qwen2-vl:8b");
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
        let job = Job::new(task_id, "qwen2-vl:8b".to_string(), vec![Uuid::new_v4()]);
        let response: JobResponse = job.into();
        let json = serde_json::to_string(&response).unwrap();

        // started_at and completed_at should not appear when None
        assert!(!json.contains("started_at"));
        assert!(!json.contains("completed_at"));
    }

    #[test]
    fn test_job_serialization() {
        let task_id = Uuid::new_v4();
        let job = Job::new(task_id, "qwen2-vl:8b".to_string(), vec![Uuid::new_v4()]);
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
}
