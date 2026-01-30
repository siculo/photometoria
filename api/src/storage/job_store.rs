//! Job storage abstraction layer
//!
//! This module provides trait-based abstraction for job persistence,
//! allowing multiple implementations (in-memory, database, etc.) without
//! changing business logic.

use async_trait::async_trait;
use thiserror::Error;

use crate::models::Job;

// ============================================================================
// Error Types
// ============================================================================

/// Errors that can occur during job storage operations.
#[derive(Debug, Error)]
pub enum JobStoreError {
    /// The requested job was not found.
    #[error("Job not found: {0}")]
    NotFound(String),

    /// A job with the same ID already exists.
    #[error("Job already exists: {0}")]
    AlreadyExists(String),

    /// A generic storage error occurred.
    #[error("Storage error: {0}")]
    #[allow(dead_code)]
    StorageError(String),
}

/// Result type alias for job storage operations.
pub type JobStoreResult<T> = Result<T, JobStoreError>;

// ============================================================================
// JobStore Trait
// ============================================================================

/// Trait defining the interface for job persistence.
///
/// This trait provides an abstract interface for storing and retrieving jobs.
/// Implementations can use different backends (in-memory, database, etc.).
///
/// ## Thread Safety
///
/// All implementations must be `Send + Sync` to support concurrent access
/// from multiple Tokio tasks without data races.
///
/// ## Relationship with Tasks
///
/// Jobs belong to Tasks (1:N relationship). Each job has a `task_id` field
/// that references its parent task. When a task is deleted, all associated
/// jobs should be deleted as well.
///
/// ## Job Lifecycle
///
/// Jobs transition through states: Queued → Processing → Completed/Failed/Cancelled.
/// The `update` method is used to persist state changes.
#[async_trait]
pub trait JobStore: Send + Sync {
    /// Creates a new job in the store.
    ///
    /// # Arguments
    /// * `job` - The job to create
    ///
    /// # Returns
    /// * `Ok(Job)` - The created job (same as input)
    /// * `Err(AlreadyExists)` - If a job with the same ID already exists
    /// * `Err(StorageError)` - If a storage-level error occurs
    async fn create(&self, job: Job) -> JobStoreResult<Job>;

    /// Retrieves a job by its ID.
    ///
    /// # Arguments
    /// * `job_id` - The unique identifier of the job
    ///
    /// # Returns
    /// * `Ok(Some(Job))` - The job if found
    /// * `Ok(None)` - If no job with the given ID exists
    /// * `Err(StorageError)` - If a storage-level error occurs
    async fn get(&self, job_id: &str) -> JobStoreResult<Option<Job>>;

    /// Lists all jobs in the store.
    ///
    /// # Returns
    /// * `Ok(Vec<Job>)` - Vector of all jobs (may be empty)
    /// * `Err(StorageError)` - If a storage-level error occurs
    ///
    /// # Note
    /// Jobs are sorted by `created_at` (oldest first) for consistent ordering.
    /// This is used by `GET /api/jobs` endpoint.
    async fn list(&self) -> JobStoreResult<Vec<Job>>;

    /// Lists all jobs belonging to a specific task.
    ///
    /// # Arguments
    /// * `task_id` - The unique identifier of the parent task
    ///
    /// # Returns
    /// * `Ok(Vec<Job>)` - Vector of jobs (may be empty)
    /// * `Err(StorageError)` - If a storage-level error occurs
    ///
    /// # Note
    /// Jobs are sorted by `created_at` (oldest first) for consistent ordering.
    async fn list_by_task(&self, task_id: &str) -> JobStoreResult<Vec<Job>>;

    /// Updates an existing job.
    ///
    /// # Arguments
    /// * `job` - The job with updated fields (job_id must match existing job)
    ///
    /// # Returns
    /// * `Ok(Job)` - The updated job
    /// * `Err(NotFound)` - If no job with the given ID exists
    /// * `Err(StorageError)` - If a storage-level error occurs
    ///
    /// # Note
    /// This is used to persist job state transitions (e.g., Queued → Processing).
    async fn update(&self, job: Job) -> JobStoreResult<Job>;

    /// Deletes a job from the store.
    ///
    /// # Arguments
    /// * `job_id` - The unique identifier of the job to delete
    ///
    /// # Returns
    /// * `Ok(())` - Job successfully deleted
    /// * `Err(NotFound)` - If no job with the given ID exists
    /// * `Err(StorageError)` - If a storage-level error occurs
    async fn delete(&self, job_id: &str) -> JobStoreResult<()>;

    /// Deletes all jobs belonging to a specific task.
    ///
    /// # Arguments
    /// * `task_id` - The unique identifier of the parent task
    ///
    /// # Returns
    /// * `Ok(usize)` - Number of jobs deleted
    /// * `Err(StorageError)` - If a storage-level error occurs
    ///
    /// # Note
    /// This is used when deleting a task to clean up all associated jobs.
    async fn delete_by_task(&self, task_id: &str) -> JobStoreResult<usize>;

    /// Checks if a job exists.
    ///
    /// # Arguments
    /// * `job_id` - The unique identifier to check
    ///
    /// # Returns
    /// * `Ok(true)` - Job exists
    /// * `Ok(false)` - Job does not exist
    /// * `Err(StorageError)` - If a storage-level error occurs
    #[allow(dead_code)]
    async fn exists(&self, job_id: &str) -> JobStoreResult<bool>;

    /// Returns the count of jobs belonging to a specific task.
    ///
    /// # Arguments
    /// * `task_id` - The unique identifier of the parent task
    ///
    /// # Returns
    /// * `Ok(usize)` - The count of jobs
    /// * `Err(StorageError)` - If a storage-level error occurs
    ///
    /// # Note
    /// This is used for TaskSummary.job_count.
    async fn count_by_task(&self, task_id: &str) -> JobStoreResult<usize>;
}
