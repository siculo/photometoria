//! Filesystem-backed implementation of JobStore
//!
//! This module provides a thread-safe implementation of the JobStore trait
//! with full persistence to the filesystem. Job metadata is stored as JSON files
//! within each task's directory.

use async_trait::async_trait;
use dashmap::DashMap;
use dashmap::mapref::entry::Entry;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::models::Job;

use super::{JobStore, JobStoreError, JobStoreResult};

// ============================================================================
// FileSystemJobStore Implementation
// ============================================================================

/// Filesystem-backed implementation of JobStore with full persistence.
///
/// This implementation stores job metadata both in memory (using DashMap for
/// fast concurrent access) and on the filesystem (as JSON files for persistence).
///
/// ## Characteristics
///
/// - **Thread-safe**: Supports concurrent access from multiple Tokio tasks
/// - **Lock-free reads**: Get operations don't acquire locks
/// - **Fine-grained locking**: Writes lock only the specific shard
/// - **Full persistence**: Job metadata survives server restarts
/// - **Nested structure**: Jobs stored in task-specific subdirectories
///
/// ## Directory Structure
///
/// ```text
/// {storage_path}/
/// └── tasks/
///     └── {task_id}/
///         ├── task.json
///         ├── photos.json
///         └── jobs/              # Jobs subdirectory
///             ├── {job_id_1}.json
///             ├── {job_id_2}.json
///             └── ...
/// ```
///
/// ## Persistence Strategy
///
/// - Metadata is written to disk after each create/update operation
/// - On startup, all existing job JSON files are loaded into memory
/// - Deleting a job removes both the in-memory entry and the JSON file
/// - Deleting a task automatically removes all job files in the task's jobs/ directory
pub struct FileSystemJobStore {
    jobs: Arc<DashMap<Uuid, Job>>,
    storage_path: PathBuf,
}

impl FileSystemJobStore {
    /// Creates a new filesystem-backed job store.
    ///
    /// This constructor loads all existing jobs from the filesystem.
    /// Any errors during loading are logged but don't prevent startup.
    ///
    /// # Arguments
    /// * `storage_path` - Base path for storing task directories
    pub async fn new(storage_path: PathBuf) -> Self {
        let store = Self {
            jobs: Arc::new(DashMap::new()),
            storage_path,
        };
        store.load_all().await;
        store
    }

    /// Returns the jobs directory path for a specific task.
    fn job_dir(&self, task_id: Uuid) -> PathBuf {
        self.storage_path
            .join("tasks")
            .join(task_id.to_string())
            .join("jobs")
    }

    /// Returns the path to a specific job's metadata file.
    fn job_json_path(&self, task_id: Uuid, job_id: Uuid) -> PathBuf {
        self.job_dir(task_id).join(format!("{}.json", job_id))
    }

    /// Loads all jobs from the filesystem into memory.
    async fn load_all(&self) {
        let tasks_dir = self.storage_path.join("tasks");

        if !tasks_dir.exists() {
            debug!("Tasks directory does not exist, starting with empty job store");
            return;
        }

        let mut task_entries = match tokio::fs::read_dir(&tasks_dir).await {
            Ok(entries) => entries,
            Err(e) => {
                warn!("Failed to read tasks directory: {}", e);
                return;
            }
        };

        let mut loaded_count = 0;
        let mut error_count = 0;

        // Iterate through each task directory
        while let Ok(Some(task_entry)) = task_entries.next_entry().await {
            let task_path = task_entry.path();
            if !task_path.is_dir() {
                continue;
            }

            let jobs_dir = task_path.join("jobs");
            if !jobs_dir.exists() {
                continue;
            }

            // Iterate through job JSON files in this task's jobs/ directory
            let mut job_entries = match tokio::fs::read_dir(&jobs_dir).await {
                Ok(entries) => entries,
                Err(e) => {
                    warn!("Failed to read jobs directory {:?}: {}", jobs_dir, e);
                    error_count += 1;
                    continue;
                }
            };

            while let Ok(Some(job_entry)) = job_entries.next_entry().await {
                let job_path = job_entry.path();
                if !job_path.is_file()
                    || job_path.extension().and_then(|s| s.to_str()) != Some("json")
                {
                    continue;
                }

                match self.load_job_from_file(&job_path).await {
                    Ok(job) => {
                        self.jobs.insert(job.job_id, job);
                        loaded_count += 1;
                    }
                    Err(e) => {
                        warn!("Failed to load job from {:?}: {}", job_path, e);
                        error_count += 1;
                    }
                }
            }
        }

        info!(
            "Loaded {} jobs from filesystem ({} errors)",
            loaded_count, error_count
        );
    }

    /// Loads a single job from a JSON file.
    async fn load_job_from_file(&self, path: &PathBuf) -> JobStoreResult<Job> {
        let content = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| JobStoreError::StorageError(format!("Failed to read job file: {}", e)))?;

        serde_json::from_str(&content)
            .map_err(|e| JobStoreError::StorageError(format!("Failed to parse job JSON: {}", e)))
    }

    /// Saves a job's metadata to the filesystem.
    async fn save_job_to_file(&self, job: &Job) -> JobStoreResult<()> {
        let path = self.job_json_path(job.task_id, job.job_id);
        let content = serde_json::to_string_pretty(job)
            .map_err(|e| JobStoreError::StorageError(format!("Failed to serialize job: {}", e)))?;

        tokio::fs::write(&path, content).await.map_err(|e| {
            error!("Failed to write job file {:?}: {}", path, e);
            JobStoreError::StorageError(format!("Failed to write job file: {}", e))
        })?;

        debug!("Saved job metadata to {:?}", path);
        Ok(())
    }
}

// ============================================================================
// JobStore Trait Implementation
// ============================================================================

#[async_trait]
impl JobStore for FileSystemJobStore {
    async fn create(&self, job: Job) -> JobStoreResult<Job> {
        let job_id = job.job_id;

        debug!("Attempting to create job: {}", job_id);

        // Use entry API to atomically check and insert
        match self.jobs.entry(job_id) {
            Entry::Occupied(_) => Err(JobStoreError::AlreadyExists(job_id)),
            Entry::Vacant(entry) => {
                // Ensure jobs directory exists
                let job_dir = self.job_dir(job.task_id);
                if let Err(e) = tokio::fs::create_dir_all(&job_dir).await {
                    error!("Failed to create jobs directory {:?}: {}", job_dir, e);
                    return Err(JobStoreError::StorageError(format!(
                        "Failed to create jobs directory: {}",
                        e
                    )));
                }
                debug!("Ensured jobs directory exists: {:?}", job_dir);

                // Save metadata to filesystem
                self.save_job_to_file(&job).await?;

                entry.insert(job.clone());
                info!(
                    "Job created successfully: {} (task: {}, model: '{}', photos: {})",
                    job_id,
                    job.task_id,
                    job.model,
                    job.photo_count()
                );
                Ok(job)
            }
        }
    }

    async fn get(&self, job_id: Uuid) -> JobStoreResult<Option<Job>> {
        debug!("Retrieving job: {}", job_id);

        // Lock-free read with DashMap
        Ok(self.jobs.get(&job_id).map(|entry| entry.value().clone()))
    }

    async fn list(&self) -> JobStoreResult<Vec<Job>> {
        debug!("Listing all jobs");

        // Collect all jobs into a vector
        let mut jobs: Vec<Job> = self
            .jobs
            .iter()
            .map(|entry| entry.value().clone())
            .collect();

        // Sort by created_at (oldest first) for deterministic output
        jobs.sort_by_key(|job| job.created_at);

        info!("Listed {} jobs", jobs.len());
        Ok(jobs)
    }

    async fn list_by_task(&self, task_id: Uuid) -> JobStoreResult<Vec<Job>> {
        debug!("Listing jobs for task: {}", task_id);

        let mut jobs: Vec<Job> = self
            .jobs
            .iter()
            .filter(|entry| entry.value().task_id == task_id)
            .map(|entry| entry.value().clone())
            .collect();

        // Sort by created_at (oldest first) for deterministic output
        jobs.sort_by_key(|job| job.created_at);

        info!("Listed {} jobs for task {}", jobs.len(), task_id);
        Ok(jobs)
    }

    async fn update(&self, job: Job) -> JobStoreResult<Job> {
        let job_id = job.job_id;

        debug!("Updating job: {}", job_id);

        // Use get_mut to modify in-place
        match self.jobs.get_mut(&job_id) {
            Some(mut entry) => {
                let old_status = entry.status;
                let old_job = entry.clone();
                *entry = job.clone();

                // Save updated metadata to filesystem
                if let Err(e) = self.save_job_to_file(&job).await {
                    // Rollback in-memory change on filesystem error
                    *entry = old_job;
                    return Err(e);
                }

                info!(
                    "Job updated successfully: {} (status: {} -> {})",
                    job_id, old_status, job.status
                );
                Ok(job)
            }
            None => Err(JobStoreError::NotFound(job_id)),
        }
    }

    async fn delete(&self, job_id: Uuid) -> JobStoreResult<()> {
        debug!("Deleting job: {}", job_id);

        // remove() returns Some((key, value)) if the entry existed
        match self.jobs.remove(&job_id) {
            Some((_, job)) => {
                // Remove job JSON file
                let job_path = self.job_json_path(job.task_id, job_id);
                if job_path.exists() {
                    if let Err(e) = tokio::fs::remove_file(&job_path).await {
                        error!("Failed to remove job file {:?}: {}", job_path, e);
                        // Log but don't fail - the job metadata is already removed from memory
                    } else {
                        debug!("Removed job file: {:?}", job_path);
                    }
                }

                info!(
                    "Job deleted successfully: {} (task: {}, status: {})",
                    job_id, job.task_id, job.status
                );
                Ok(())
            }
            None => Err(JobStoreError::NotFound(job_id)),
        }
    }

    async fn delete_by_task(&self, task_id: Uuid) -> JobStoreResult<usize> {
        debug!("Deleting all jobs for task: {}", task_id);

        // Collect job_ids to delete (can't delete while iterating)
        let job_ids: Vec<Uuid> = self
            .jobs
            .iter()
            .filter(|entry| entry.value().task_id == task_id)
            .map(|entry| *entry.key())
            .collect();

        let count = job_ids.len();

        // Remove all jobs and their files
        for job_id in job_ids {
            if let Some((_, job)) = self.jobs.remove(&job_id) {
                let job_path = self.job_json_path(job.task_id, job_id);
                if job_path.exists() {
                    if let Err(e) = tokio::fs::remove_file(&job_path).await {
                        error!("Failed to remove job file {:?}: {}", job_path, e);
                    } else {
                        debug!("Removed job file: {:?}", job_path);
                    }
                }
            }
        }

        info!("Deleted {} jobs for task {}", count, task_id);
        Ok(count)
    }

    async fn exists(&self, job_id: Uuid) -> JobStoreResult<bool> {
        debug!("Checking if job exists: {}", job_id);

        // Lock-free existence check
        Ok(self.jobs.contains_key(&job_id))
    }

    async fn count_by_task(&self, task_id: Uuid) -> JobStoreResult<usize> {
        debug!("Counting jobs for task: {}", task_id);

        let count = self
            .jobs
            .iter()
            .filter(|entry| entry.value().task_id == task_id)
            .count();

        Ok(count)
    }
}

// ============================================================================
// Unit Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::JobStatus;
    use chrono::{DateTime, Duration, Utc};
    use tempfile::TempDir;
    use uuid::Uuid;

    // Helper struct to keep temp dir alive during test
    struct TestStore {
        store: FileSystemJobStore,
        _temp_dir: TempDir,
    }

    // Helper function to create a fresh store with temp directory
    async fn create_store() -> TestStore {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let store = FileSystemJobStore::new(temp_dir.path().to_path_buf()).await;
        TestStore {
            store,
            _temp_dir: temp_dir,
        }
    }

    // Helper function to create a test job
    fn create_test_job(task_id: Uuid, model: &str, photo_ids: Vec<Uuid>) -> Job {
        Job::new(task_id, model.to_string(), photo_ids)
    }

    // Helper function to create a test job with specific timestamp
    fn create_test_job_with_timestamp(
        task_id: Uuid,
        model: &str,
        photo_ids: Vec<Uuid>,
        timestamp: DateTime<Utc>,
    ) -> Job {
        let job = Job::new(task_id, model.to_string(), photo_ids);
        // Use a hack to set the timestamp via serialization
        let mut job_value = serde_json::to_value(&job).unwrap();
        job_value["created_at"] = serde_json::to_value(timestamp).unwrap();
        serde_json::from_value(job_value).unwrap()
    }

    #[tokio::test]
    async fn test_create_job() {
        let ts = create_store().await;
        let task_id = Uuid::new_v4();
        let photo_ids = vec![Uuid::new_v4(), Uuid::new_v4()];
        let job = create_test_job(task_id, "qwen2-vl:8b", photo_ids);

        let result = ts.store.create(job.clone()).await;

        assert!(result.is_ok());
        let created = result.unwrap();
        assert_eq!(created.job_id, job.job_id);
        assert_eq!(created.task_id, job.task_id);
        assert_eq!(created.model, job.model);
        assert_eq!(created.status, JobStatus::Queued);

        // Verify it exists in the store
        let exists = ts.store.exists(job.job_id).await.unwrap();
        assert!(exists);

        // Verify jobs directory was created
        assert!(ts.store.job_dir(task_id).exists());

        // Verify job JSON file was created
        assert!(ts.store.job_json_path(task_id, job.job_id).exists());
    }

    #[tokio::test]
    async fn test_create_duplicate_fails() {
        let ts = create_store().await;
        let task_id = Uuid::new_v4();
        let photo_ids = vec![Uuid::new_v4()];
        let job = create_test_job(task_id, "qwen2-vl:8b", photo_ids);

        // First creation should succeed
        ts.store.create(job.clone()).await.unwrap();

        // Second creation with same ID should fail
        let result = ts.store.create(job.clone()).await;

        assert!(result.is_err());
        match result {
            Err(JobStoreError::AlreadyExists(id)) => {
                assert_eq!(id, job.job_id);
            }
            _ => panic!("Expected AlreadyExists error"),
        }
    }

    #[tokio::test]
    async fn test_get_existing_job() {
        let ts = create_store().await;
        let task_id = Uuid::new_v4();
        let photo_ids = vec![Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()];
        let job = create_test_job(task_id, "llava", photo_ids);

        ts.store.create(job.clone()).await.unwrap();

        let result = ts.store.get(job.job_id).await.unwrap();

        assert!(result.is_some());
        let retrieved = result.unwrap();
        assert_eq!(retrieved.job_id, job.job_id);
        assert_eq!(retrieved.task_id, job.task_id);
        assert_eq!(retrieved.model, job.model);
        assert_eq!(retrieved.photo_count(), 3);
    }

    #[tokio::test]
    async fn test_get_nonexistent_job() {
        let ts = create_store().await;

        let result = ts.store.get(Uuid::new_v4()).await.unwrap();

        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_list_jobs_ordered_by_created_at() {
        let ts = create_store().await;
        let task_id = Uuid::new_v4();

        // Create jobs with explicit timestamps to control ordering
        let now = Utc::now();
        let job1 =
            create_test_job_with_timestamp(task_id, "qwen2-vl:8b", vec![Uuid::new_v4()], now);
        let job2 = create_test_job_with_timestamp(
            task_id,
            "llava",
            vec![Uuid::new_v4()],
            now + Duration::seconds(1),
        );
        let job3 = create_test_job_with_timestamp(
            task_id,
            "qwen2-vl:8b",
            vec![Uuid::new_v4()],
            now + Duration::seconds(2),
        );

        // Insert in random order
        ts.store.create(job2.clone()).await.unwrap();
        ts.store.create(job1.clone()).await.unwrap();
        ts.store.create(job3.clone()).await.unwrap();

        // List should return ordered by created_at (oldest first)
        let jobs = ts.store.list().await.unwrap();

        assert_eq!(jobs.len(), 3);
        assert_eq!(jobs[0].job_id, job1.job_id);
        assert_eq!(jobs[1].job_id, job2.job_id);
        assert_eq!(jobs[2].job_id, job3.job_id);
    }

    #[tokio::test]
    async fn test_list_by_task() {
        let ts = create_store().await;

        // Create jobs for two different tasks
        let task_a = Uuid::new_v4();
        let task_b = Uuid::new_v4();
        let task_c = Uuid::new_v4();
        let photo_id = Uuid::new_v4();
        let job1 = create_test_job(task_a, "qwen2-vl:8b", vec![photo_id]);
        let job2 = create_test_job(task_a, "llava", vec![photo_id]);
        let job3 = create_test_job(task_b, "qwen2-vl:8b", vec![Uuid::new_v4()]);

        ts.store.create(job1).await.unwrap();
        ts.store.create(job2).await.unwrap();
        ts.store.create(job3).await.unwrap();

        // List jobs for task_a
        let jobs_a = ts.store.list_by_task(task_a).await.unwrap();
        assert_eq!(jobs_a.len(), 2);
        assert!(jobs_a.iter().all(|j| j.task_id == task_a));

        // List jobs for task_b
        let jobs_b = ts.store.list_by_task(task_b).await.unwrap();
        assert_eq!(jobs_b.len(), 1);
        assert_eq!(jobs_b[0].task_id, task_b);

        // List jobs for nonexistent task
        let jobs_c = ts.store.list_by_task(task_c).await.unwrap();
        assert!(jobs_c.is_empty());
    }

    #[tokio::test]
    async fn test_update_job() {
        let ts = create_store().await;
        let task_id = Uuid::new_v4();
        let mut job = create_test_job(task_id, "qwen2-vl:8b", vec![Uuid::new_v4()]);

        ts.store.create(job.clone()).await.unwrap();

        // Update the job status
        job.start();
        let result = ts.store.update(job.clone()).await;

        assert!(result.is_ok());
        let updated = result.unwrap();
        assert_eq!(updated.status, JobStatus::Processing);
        assert!(updated.started_at.is_some());

        // Verify the change persisted
        let retrieved = ts.store.get(job.job_id).await.unwrap().unwrap();
        assert_eq!(retrieved.status, JobStatus::Processing);
    }

    #[tokio::test]
    async fn test_update_nonexistent_fails() {
        let ts = create_store().await;
        let task_id = Uuid::new_v4();
        let job = create_test_job(task_id, "qwen2-vl:8b", vec![Uuid::new_v4()]);

        // Try to update without creating first
        let result = ts.store.update(job.clone()).await;

        assert!(result.is_err());
        match result {
            Err(JobStoreError::NotFound(id)) => {
                assert_eq!(id, job.job_id);
            }
            _ => panic!("Expected NotFound error"),
        }
    }

    #[tokio::test]
    async fn test_delete_job() {
        let ts = create_store().await;
        let task_id = Uuid::new_v4();
        let job = create_test_job(task_id, "qwen2-vl:8b", vec![Uuid::new_v4()]);

        ts.store.create(job.clone()).await.unwrap();
        let job_path = ts.store.job_json_path(task_id, job.job_id);
        assert!(job_path.exists());

        // Delete the job
        let result = ts.store.delete(job.job_id).await;

        assert!(result.is_ok());

        // Verify it no longer exists
        let retrieved = ts.store.get(job.job_id).await.unwrap();
        assert!(retrieved.is_none());

        // Verify file was removed
        assert!(!job_path.exists());
    }

    #[tokio::test]
    async fn test_delete_nonexistent_fails() {
        let ts = create_store().await;

        let nonexistent_id = Uuid::new_v4();
        let result = ts.store.delete(nonexistent_id).await;

        assert!(result.is_err());
        match result {
            Err(JobStoreError::NotFound(id)) => {
                assert_eq!(id, nonexistent_id);
            }
            _ => panic!("Expected NotFound error"),
        }
    }

    #[tokio::test]
    async fn test_delete_by_task() {
        let ts = create_store().await;

        // Create jobs for two different tasks
        let task_a = Uuid::new_v4();
        let task_b = Uuid::new_v4();
        let photo_id = Uuid::new_v4();
        let job1 = create_test_job(task_a, "qwen2-vl:8b", vec![photo_id]);
        let job2 = create_test_job(task_a, "llava", vec![photo_id]);
        let job3 = create_test_job(task_b, "qwen2-vl:8b", vec![Uuid::new_v4()]);

        ts.store.create(job1.clone()).await.unwrap();
        ts.store.create(job2.clone()).await.unwrap();
        ts.store.create(job3).await.unwrap();

        let job1_path = ts.store.job_json_path(task_a, job1.job_id);
        let job2_path = ts.store.job_json_path(task_a, job2.job_id);
        assert!(job1_path.exists());
        assert!(job2_path.exists());

        // Delete all jobs for task_a
        let deleted = ts.store.delete_by_task(task_a).await.unwrap();
        assert_eq!(deleted, 2);

        // Verify task_a jobs are gone
        let jobs_a = ts.store.list_by_task(task_a).await.unwrap();
        assert!(jobs_a.is_empty());

        // Verify files were removed
        assert!(!job1_path.exists());
        assert!(!job2_path.exists());

        // Verify task_b job still exists
        let jobs_b = ts.store.list_by_task(task_b).await.unwrap();
        assert_eq!(jobs_b.len(), 1);
    }

    #[tokio::test]
    async fn test_exists() {
        let ts = create_store().await;
        let task_id = Uuid::new_v4();
        let job = create_test_job(task_id, "qwen2-vl:8b", vec![Uuid::new_v4()]);

        // Should not exist initially
        let exists = ts.store.exists(job.job_id).await.unwrap();
        assert!(!exists);

        // Create the job
        ts.store.create(job.clone()).await.unwrap();

        // Should exist now
        let exists = ts.store.exists(job.job_id).await.unwrap();
        assert!(exists);

        // Random ID should not exist
        let exists = ts.store.exists(Uuid::new_v4()).await.unwrap();
        assert!(!exists);
    }

    #[tokio::test]
    async fn test_count_by_task() {
        let ts = create_store().await;

        let task_a = Uuid::new_v4();
        let task_b = Uuid::new_v4();

        // Empty task should have count 0
        let count = ts.store.count_by_task(task_a).await.unwrap();
        assert_eq!(count, 0);

        // Add jobs
        let photo_id = Uuid::new_v4();
        let job1 = create_test_job(task_a, "qwen2-vl:8b", vec![photo_id]);
        let job2 = create_test_job(task_a, "llava", vec![photo_id]);
        let job3 = create_test_job(task_b, "qwen2-vl:8b", vec![Uuid::new_v4()]);

        ts.store.create(job1).await.unwrap();
        ts.store.create(job2).await.unwrap();
        ts.store.create(job3).await.unwrap();

        // Count for task_a
        let count = ts.store.count_by_task(task_a).await.unwrap();
        assert_eq!(count, 2);

        // Count for task_b
        let count = ts.store.count_by_task(task_b).await.unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn test_job_lifecycle_through_store() {
        let ts = create_store().await;
        let task_id = Uuid::new_v4();
        let mut job = create_test_job(task_id, "qwen2-vl:8b", vec![Uuid::new_v4(), Uuid::new_v4()]);

        // Create job (Queued)
        ts.store.create(job.clone()).await.unwrap();
        let retrieved = ts.store.get(job.job_id).await.unwrap().unwrap();
        assert_eq!(retrieved.status, JobStatus::Queued);

        // Start job (Processing)
        job.start();
        ts.store.update(job.clone()).await.unwrap();
        let retrieved = ts.store.get(job.job_id).await.unwrap().unwrap();
        assert_eq!(retrieved.status, JobStatus::Processing);
        assert!(retrieved.started_at.is_some());

        // Complete job
        job.complete();
        ts.store.update(job.clone()).await.unwrap();
        let retrieved = ts.store.get(job.job_id).await.unwrap().unwrap();
        assert_eq!(retrieved.status, JobStatus::Completed);
        assert!(retrieved.completed_at.is_some());
        assert!(retrieved.is_finished());
    }

    // ========================================================================
    // Persistence Tests
    // ========================================================================

    #[tokio::test]
    async fn test_persistence_survives_reload() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let storage_path = temp_dir.path().to_path_buf();

        // Create store and add jobs
        let task_id = Uuid::new_v4();
        let job1 = create_test_job(task_id, "qwen2-vl:8b", vec![Uuid::new_v4()]);
        let job2 = create_test_job(task_id, "llava", vec![Uuid::new_v4()]);
        let job1_id = job1.job_id;
        let job2_id = job2.job_id;

        {
            let store = FileSystemJobStore::new(storage_path.clone()).await;
            store.create(job1).await.unwrap();
            store.create(job2).await.unwrap();
            assert_eq!(store.count_by_task(task_id).await.unwrap(), 2);
        }

        // Create new store instance (simulates server restart)
        let store = FileSystemJobStore::new(storage_path).await;

        // Jobs should be loaded from filesystem
        assert_eq!(store.count_by_task(task_id).await.unwrap(), 2);
        assert!(store.exists(job1_id).await.unwrap());
        assert!(store.exists(job2_id).await.unwrap());

        let loaded_job1 = store.get(job1_id).await.unwrap().unwrap();
        assert_eq!(loaded_job1.model, "qwen2-vl:8b");
    }

    #[tokio::test]
    async fn test_update_persists() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let storage_path = temp_dir.path().to_path_buf();

        let task_id = Uuid::new_v4();
        let mut job = create_test_job(task_id, "qwen2-vl:8b", vec![Uuid::new_v4()]);
        let job_id = job.job_id;

        {
            let store = FileSystemJobStore::new(storage_path.clone()).await;
            store.create(job.clone()).await.unwrap();

            // Update the job
            job.start();
            store.update(job.clone()).await.unwrap();
        }

        // Reload and verify update persisted
        let store = FileSystemJobStore::new(storage_path).await;
        let loaded = store.get(job_id).await.unwrap().unwrap();
        assert_eq!(loaded.status, JobStatus::Processing);
        assert!(loaded.started_at.is_some());
    }

    #[tokio::test]
    async fn test_delete_removes_from_filesystem() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let storage_path = temp_dir.path().to_path_buf();

        let task_id = Uuid::new_v4();
        let job = create_test_job(task_id, "qwen2-vl:8b", vec![Uuid::new_v4()]);
        let job_id = job.job_id;

        {
            let store = FileSystemJobStore::new(storage_path.clone()).await;
            store.create(job).await.unwrap();
            store.delete(job_id).await.unwrap();
        }

        // Reload and verify job is gone
        let store = FileSystemJobStore::new(storage_path).await;
        assert_eq!(store.count_by_task(task_id).await.unwrap(), 0);
        assert!(!store.exists(job_id).await.unwrap());
    }

    #[tokio::test]
    async fn test_multiple_tasks_with_jobs_load_correctly() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let storage_path = temp_dir.path().to_path_buf();

        // Create multiple tasks with multiple jobs each
        let task_a = Uuid::new_v4();
        let task_b = Uuid::new_v4();
        let job1 = create_test_job(task_a, "qwen2-vl:8b", vec![Uuid::new_v4()]);
        let job2 = create_test_job(task_a, "llava", vec![Uuid::new_v4()]);
        let job3 = create_test_job(task_b, "qwen2-vl:8b", vec![Uuid::new_v4()]);
        let job4 = create_test_job(task_b, "llava", vec![Uuid::new_v4()]);

        {
            let store = FileSystemJobStore::new(storage_path.clone()).await;
            store.create(job1).await.unwrap();
            store.create(job2).await.unwrap();
            store.create(job3).await.unwrap();
            store.create(job4).await.unwrap();
        }

        // Reload and verify all jobs loaded correctly
        let store = FileSystemJobStore::new(storage_path).await;
        assert_eq!(store.list().await.unwrap().len(), 4);
        assert_eq!(store.count_by_task(task_a).await.unwrap(), 2);
        assert_eq!(store.count_by_task(task_b).await.unwrap(), 2);
    }
}
