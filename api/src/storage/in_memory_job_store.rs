//! In-memory implementation of JobStore using DashMap
//!
//! This module provides a thread-safe in-memory implementation of the JobStore trait
//! using DashMap for concurrent access without global locks.

use async_trait::async_trait;
use dashmap::DashMap;
use dashmap::mapref::entry::Entry;
use std::sync::Arc;
use tracing::{debug, info};
use uuid::Uuid;

use crate::models::Job;

use super::{JobStore, JobStoreError, JobStoreResult};

// ============================================================================
// InMemoryJobStore Implementation
// ============================================================================

/// In-memory implementation of JobStore using DashMap for thread-safe concurrent access.
///
/// This implementation stores jobs in memory using DashMap, which provides excellent
/// concurrent read/write performance through internal sharding.
///
/// ## Characteristics
///
/// - **Thread-safe**: Supports concurrent access from multiple Tokio tasks
/// - **Lock-free reads**: Get operations don't acquire locks
/// - **Fine-grained locking**: Writes lock only the specific shard
/// - **No persistence**: Data is lost when the server restarts
/// - **Flat structure**: All jobs in a single map, filtered by task_id when needed
///
/// ## Use Cases
///
/// - Development and testing
/// - Single-user scenarios
/// - Prototyping before adding database persistence
pub struct InMemoryJobStore {
    jobs: Arc<DashMap<Uuid, Job>>,
}

impl InMemoryJobStore {
    /// Creates a new empty in-memory job store.
    pub fn new() -> Self {
        Self {
            jobs: Arc::new(DashMap::new()),
        }
    }
}

impl Default for InMemoryJobStore {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// JobStore Trait Implementation
// ============================================================================

#[async_trait]
impl JobStore for InMemoryJobStore {
    async fn create(&self, job: Job) -> JobStoreResult<Job> {
        let job_id = job.job_id;

        debug!("Attempting to create job: {}", job_id);

        match self.jobs.entry(job_id) {
            Entry::Occupied(_) => Err(JobStoreError::AlreadyExists(job_id)),
            Entry::Vacant(entry) => {
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

        Ok(self.jobs.get(&job_id).map(|entry| entry.value().clone()))
    }

    async fn list(&self) -> JobStoreResult<Vec<Job>> {
        debug!("Listing all jobs");

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

        match self.jobs.get_mut(&job_id) {
            Some(mut entry) => {
                let old_status = entry.status;
                *entry = job.clone();
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

        match self.jobs.remove(&job_id) {
            Some((_, job)) => {
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

        for job_id in job_ids {
            self.jobs.remove(&job_id);
        }

        info!("Deleted {} jobs for task {}", count, task_id);
        Ok(count)
    }

    async fn exists(&self, job_id: Uuid) -> JobStoreResult<bool> {
        debug!("Checking if job exists: {}", job_id);

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

    // Helper function to create a fresh store
    fn create_store() -> InMemoryJobStore {
        InMemoryJobStore::new()
    }

    // Helper function to create a test job
    fn create_test_job(task_id: Uuid, model: &str, photo_ids: Vec<Uuid>) -> Job {
        Job::new(task_id, model.to_string(), photo_ids)
    }

    #[tokio::test]
    async fn test_create_job() {
        let store = create_store();
        let task_id = Uuid::new_v4();
        let photo_ids = vec![Uuid::new_v4(), Uuid::new_v4()];
        let job = create_test_job(task_id, "qwen2-vl:8b", photo_ids);

        let result = store.create(job.clone()).await;

        assert!(result.is_ok());
        let created = result.unwrap();
        assert_eq!(created.job_id, job.job_id);
        assert_eq!(created.task_id, job.task_id);
        assert_eq!(created.model, job.model);
        assert_eq!(created.status, JobStatus::Queued);

        // Verify it exists in the store
        let exists = store.exists(job.job_id).await.unwrap();
        assert!(exists);
    }

    #[tokio::test]
    async fn test_create_duplicate_fails() {
        let store = create_store();
        let task_id = Uuid::new_v4();
        let photo_ids = vec![Uuid::new_v4()];
        let job = create_test_job(task_id, "qwen2-vl:8b", photo_ids);

        // First creation should succeed
        store.create(job.clone()).await.unwrap();

        // Second creation with same ID should fail
        let result = store.create(job.clone()).await;

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
        let store = create_store();
        let task_id = Uuid::new_v4();
        let photo_ids = vec![Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()];
        let job = create_test_job(task_id, "llava", photo_ids);

        store.create(job.clone()).await.unwrap();

        let result = store.get(job.job_id).await.unwrap();

        assert!(result.is_some());
        let retrieved = result.unwrap();
        assert_eq!(retrieved.job_id, job.job_id);
        assert_eq!(retrieved.task_id, job.task_id);
        assert_eq!(retrieved.model, job.model);
        assert_eq!(retrieved.photo_count(), 3);
    }

    #[tokio::test]
    async fn test_get_nonexistent_job() {
        let store = create_store();

        let result = store.get(Uuid::new_v4()).await.unwrap();

        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_list_all_jobs() {
        let store = create_store();

        // Create jobs for different tasks
        let task_a = Uuid::new_v4();
        let task_b = Uuid::new_v4();
        let job1 = create_test_job(task_a, "qwen2-vl:8b", vec![Uuid::new_v4()]);
        let job2 = create_test_job(task_b, "llava", vec![Uuid::new_v4()]);

        store.create(job1.clone()).await.unwrap();
        store.create(job2.clone()).await.unwrap();

        let jobs = store.list().await.unwrap();

        assert_eq!(jobs.len(), 2);
    }

    #[tokio::test]
    async fn test_list_by_task() {
        let store = create_store();

        // Create jobs for two different tasks
        let task_a = Uuid::new_v4();
        let task_b = Uuid::new_v4();
        let task_c = Uuid::new_v4();
        let photo_id = Uuid::new_v4();
        let job1 = create_test_job(task_a, "qwen2-vl:8b", vec![photo_id]);
        let job2 = create_test_job(task_a, "llava", vec![photo_id]);
        let job3 = create_test_job(task_b, "qwen2-vl:8b", vec![Uuid::new_v4()]);

        store.create(job1).await.unwrap();
        store.create(job2).await.unwrap();
        store.create(job3).await.unwrap();

        // List jobs for task_a
        let jobs_a = store.list_by_task(task_a).await.unwrap();
        assert_eq!(jobs_a.len(), 2);
        assert!(jobs_a.iter().all(|j| j.task_id == task_a));

        // List jobs for task_b
        let jobs_b = store.list_by_task(task_b).await.unwrap();
        assert_eq!(jobs_b.len(), 1);
        assert_eq!(jobs_b[0].task_id, task_b);

        // List jobs for nonexistent task
        let jobs_c = store.list_by_task(task_c).await.unwrap();
        assert!(jobs_c.is_empty());
    }

    #[tokio::test]
    async fn test_update_job() {
        let store = create_store();
        let task_id = Uuid::new_v4();
        let mut job = create_test_job(task_id, "qwen2-vl:8b", vec![Uuid::new_v4()]);

        store.create(job.clone()).await.unwrap();

        // Update the job status
        job.start();
        let result = store.update(job.clone()).await;

        assert!(result.is_ok());
        let updated = result.unwrap();
        assert_eq!(updated.status, JobStatus::Processing);
        assert!(updated.started_at.is_some());

        // Verify the change persisted
        let retrieved = store.get(job.job_id).await.unwrap().unwrap();
        assert_eq!(retrieved.status, JobStatus::Processing);
    }

    #[tokio::test]
    async fn test_update_nonexistent_fails() {
        let store = create_store();
        let task_id = Uuid::new_v4();
        let job = create_test_job(task_id, "qwen2-vl:8b", vec![Uuid::new_v4()]);

        // Try to update without creating first
        let result = store.update(job.clone()).await;

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
        let store = create_store();
        let task_id = Uuid::new_v4();
        let job = create_test_job(task_id, "qwen2-vl:8b", vec![Uuid::new_v4()]);

        store.create(job.clone()).await.unwrap();

        // Delete the job
        let result = store.delete(job.job_id).await;

        assert!(result.is_ok());

        // Verify it no longer exists
        let retrieved = store.get(job.job_id).await.unwrap();
        assert!(retrieved.is_none());
    }

    #[tokio::test]
    async fn test_delete_nonexistent_fails() {
        let store = create_store();

        let nonexistent_id = Uuid::new_v4();
        let result = store.delete(nonexistent_id).await;

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
        let store = create_store();

        // Create jobs for two different tasks
        let task_a = Uuid::new_v4();
        let task_b = Uuid::new_v4();
        let photo_id = Uuid::new_v4();
        let job1 = create_test_job(task_a, "qwen2-vl:8b", vec![photo_id]);
        let job2 = create_test_job(task_a, "llava", vec![photo_id]);
        let job3 = create_test_job(task_b, "qwen2-vl:8b", vec![Uuid::new_v4()]);

        store.create(job1).await.unwrap();
        store.create(job2).await.unwrap();
        store.create(job3).await.unwrap();

        // Delete all jobs for task_a
        let deleted = store.delete_by_task(task_a).await.unwrap();
        assert_eq!(deleted, 2);

        // Verify task_a jobs are gone
        let jobs_a = store.list_by_task(task_a).await.unwrap();
        assert!(jobs_a.is_empty());

        // Verify task_b job still exists
        let jobs_b = store.list_by_task(task_b).await.unwrap();
        assert_eq!(jobs_b.len(), 1);
    }

    #[tokio::test]
    async fn test_count_by_task() {
        let store = create_store();

        let task_a = Uuid::new_v4();
        let task_b = Uuid::new_v4();

        // Empty task should have count 0
        let count = store.count_by_task(task_a).await.unwrap();
        assert_eq!(count, 0);

        // Add jobs
        let photo_id = Uuid::new_v4();
        let job1 = create_test_job(task_a, "qwen2-vl:8b", vec![photo_id]);
        let job2 = create_test_job(task_a, "llava", vec![photo_id]);
        let job3 = create_test_job(task_b, "qwen2-vl:8b", vec![Uuid::new_v4()]);

        store.create(job1).await.unwrap();
        store.create(job2).await.unwrap();
        store.create(job3).await.unwrap();

        // Count for task_a
        let count = store.count_by_task(task_a).await.unwrap();
        assert_eq!(count, 2);

        // Count for task_b
        let count = store.count_by_task(task_b).await.unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn test_exists() {
        let store = create_store();
        let task_id = Uuid::new_v4();
        let job = create_test_job(task_id, "qwen2-vl:8b", vec![Uuid::new_v4()]);

        // Should not exist initially
        let exists = store.exists(job.job_id).await.unwrap();
        assert!(!exists);

        // Create the job
        store.create(job.clone()).await.unwrap();

        // Should exist now
        let exists = store.exists(job.job_id).await.unwrap();
        assert!(exists);

        // Random ID should not exist
        let exists = store.exists(Uuid::new_v4()).await.unwrap();
        assert!(!exists);
    }

    #[tokio::test]
    async fn test_job_lifecycle_through_store() {
        let store = create_store();
        let task_id = Uuid::new_v4();
        let mut job = create_test_job(task_id, "qwen2-vl:8b", vec![Uuid::new_v4(), Uuid::new_v4()]);

        // Create job (Queued)
        store.create(job.clone()).await.unwrap();
        let retrieved = store.get(job.job_id).await.unwrap().unwrap();
        assert_eq!(retrieved.status, JobStatus::Queued);

        // Start job (Processing)
        job.start();
        store.update(job.clone()).await.unwrap();
        let retrieved = store.get(job.job_id).await.unwrap().unwrap();
        assert_eq!(retrieved.status, JobStatus::Processing);
        assert!(retrieved.started_at.is_some());

        // Complete job
        job.complete();
        store.update(job.clone()).await.unwrap();
        let retrieved = store.get(job.job_id).await.unwrap().unwrap();
        assert_eq!(retrieved.status, JobStatus::Completed);
        assert!(retrieved.completed_at.is_some());
        assert!(retrieved.is_finished());
    }
}
