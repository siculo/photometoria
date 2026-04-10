// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 The Photometoria contributors

//! Filesystem-backed implementation of JobStore
//!
//! This module provides a thread-safe implementation of the JobStore trait
//! with full persistence to the filesystem. Job metadata is stored as JSON files
//! within each task's directory.

use async_trait::async_trait;
use chrono::Local;
use dashmap::DashMap;
use dashmap::mapref::entry::Entry;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::models::Job;

use super::utils::{load_json_from_file, parse_uuid_from_dir, try_quarantine, try_scan_task_dirs};
use super::{FileSystemLayout, JobStore, JobStoreError, JobStoreResult, TaskStore};

fn sorted_jobs(iter: impl Iterator<Item = Job>) -> Vec<Job> {
    let mut jobs: Vec<Job> = iter.collect();
    jobs.sort_by_key(|j| j.created_at);
    jobs
}

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
/// ## Persistence Strategy
///
/// - Metadata is written to disk after each create/update operation
/// - On startup, all existing job JSON files are loaded into memory
/// - Deleting a job removes both the in-memory entry and the JSON file
/// - Deleting a task automatically removes all job files in the task's jobs/ directory
///
/// For details on the filesystem layout, see [`FileSystemLayout`]
pub struct FileSystemJobStore {
    jobs: Arc<DashMap<Uuid, Job>>,
    layout: FileSystemLayout,
    task_store: Arc<dyn TaskStore>,
}

impl FileSystemJobStore {
    /// Creates a new filesystem-backed job store.
    ///
    /// This constructor loads all existing jobs from the filesystem.
    /// Any errors during loading are logged but don't prevent startup.
    ///
    /// # Arguments
    /// * `storage_path` - Base path for storing task directories
    /// * `task_store` - Reference to the task store for resolving task-to-catalog relationships
    pub async fn new(storage_path: PathBuf, task_store: Arc<dyn TaskStore>) -> Self {
        Self::new_with_boot_ts(
            storage_path,
            task_store,
            Local::now().format("%Y%m%d_%H%M%S").to_string(),
        )
        .await
    }

    /// Creates a new filesystem-backed job store with an explicit boot timestamp.
    ///
    /// Use this when multiple stores must share the same quarantine directory for
    /// a single server boot.
    pub async fn new_with_boot_ts(
        storage_path: PathBuf,
        task_store: Arc<dyn TaskStore>,
        boot_ts: String,
    ) -> Self {
        let store = Self {
            jobs: Arc::new(DashMap::new()),
            layout: FileSystemLayout::new_with_boot_ts(storage_path, boot_ts),
            task_store,
        };
        store.load_all().await;
        store
    }

    /// Resolves the catalog identity for a given task.
    async fn resolve_catalog_id(&self, task_id: Uuid) -> JobStoreResult<Uuid> {
        let task = self
            .task_store
            .get(task_id)
            .await
            .map_err(|e| JobStoreError::StorageError(format!("Failed to query task store: {}", e)))?
            .ok_or_else(|| {
                JobStoreError::StorageError(format!(
                    "Cannot resolve catalog: task {} not found",
                    task_id
                ))
            })?;
        Ok(task.catalog_id)
    }

    /// Loads all jobs from the filesystem into memory.
    ///
    /// Scans the catalog hierarchy: `catalogs/{catalog_id}/tasks/{task_id}/jobs/*.json`
    async fn load_all(&self) {
        let catalog_dirs = match self.layout.scan_catalog_dirs().await {
            Ok(dirs) => dirs,
            Err(e) => {
                warn!("Failed to scan catalog directories: {}", e);
                return;
            }
        };

        let mut loaded = 0;
        let mut errors = 0;

        for catalog_dir in catalog_dirs {
            let Some(catalog_id) = parse_uuid_from_dir(&catalog_dir) else {
                warn!("Skipping invalid catalog directory: {:?}", catalog_dir);
                continue;
            };
            let (n_loaded, n_errors) = self.load_catalog_jobs(catalog_id).await;
            loaded += n_loaded;
            errors += n_errors;
        }

        info!("Loaded {} jobs from filesystem ({} errors)", loaded, errors);
    }

    /// Loads all jobs for a single catalog from the filesystem.
    ///
    /// Returns the count of successfully loaded jobs and errors encountered.
    /// Job files with corrupt, inconsistent, or unresolvable data are moved to
    /// the boot-scoped quarantine directory.
    async fn load_catalog_jobs(&self, catalog_id: Uuid) -> (usize, usize) {
        let Some(task_dirs) = try_scan_task_dirs(&self.layout, catalog_id).await else {
            return (0, 1);
        };

        let mut loaded = 0;
        let mut errors = 0;

        for task_dir in task_dirs {
            let Some(task_id) = parse_uuid_from_dir(&task_dir) else {
                warn!("Skipping invalid task directory: {:?}", task_dir);
                continue;
            };

            let job_files = match self.layout.scan_job_files(catalog_id, task_id).await {
                Ok(files) => files,
                Err(e) => {
                    warn!("Failed to scan jobs for task {}: {}", task_id, e);
                    errors += 1;
                    continue;
                }
            };

            for job_path in job_files {
                let Some(filename_id) = job_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .and_then(|s| s.parse::<Uuid>().ok())
                else {
                    warn!("Skipping non-UUID job file: {:?}", job_path);
                    continue;
                };

                let job = match load_json_from_file::<Job>(&job_path).await {
                    Ok(j) => j,
                    Err(e) => {
                        error!("Corrupt job file {:?}: {} — quarantining", job_path, e);
                        try_quarantine(&self.layout, &job_path).await;
                        errors += 1;
                        continue;
                    }
                };

                if filename_id != job.job_id {
                    error!(
                        "Inconsistent job file {:?}: job_id {} does not match filename {} — quarantining",
                        job_path, job.job_id, filename_id
                    );
                    try_quarantine(&self.layout, &job_path).await;
                    errors += 1;
                    continue;
                }

                if job.task_id != task_id {
                    error!(
                        "Inconsistent job file {:?}: task_id {} does not match directory {} — quarantining",
                        job_path, job.task_id, task_id
                    );
                    try_quarantine(&self.layout, &job_path).await;
                    errors += 1;
                    continue;
                }

                self.jobs.insert(job.job_id, job);
                loaded += 1;
            }
        }

        (loaded, errors)
    }

    /// Saves a job's metadata to the filesystem atomically.
    ///
    /// Writes to a temporary file first, then renames to the final path.
    /// This ensures the target file is never left in a partially-written state.
    async fn save_job_to_file(&self, job: &Job, catalog_id: Uuid) -> JobStoreResult<()> {
        let path = self
            .layout
            .job_file_path(catalog_id, job.task_id, job.job_id);
        let tmp_path = path.with_extension("tmp");
        let content = serde_json::to_string_pretty(job)
            .map_err(|e| JobStoreError::StorageError(format!("Failed to serialize job: {}", e)))?;

        tokio::fs::write(&tmp_path, &content).await.map_err(|e| {
            error!("Failed to write temporary job file {:?}: {}", tmp_path, e);
            JobStoreError::StorageError(format!("Failed to write job file: {}", e))
        })?;

        tokio::fs::rename(&tmp_path, &path).await.map_err(|e| {
            error!("Failed to rename {:?} -> {:?}: {}", tmp_path, path, e);
            JobStoreError::StorageError(format!("Failed to atomically save job file: {}", e))
        })?;

        debug!("Saved job metadata to {:?}", path);
        Ok(())
    }
}

#[async_trait]
impl JobStore for FileSystemJobStore {
    async fn create(&self, job: Job) -> JobStoreResult<Job> {
        let job_id = job.job_id;

        debug!("Attempting to create job: {}", job_id);

        let catalog_id = self.resolve_catalog_id(job.task_id).await?;

        match self.jobs.entry(job_id) {
            Entry::Occupied(_) => Err(JobStoreError::AlreadyExists(job_id)),
            Entry::Vacant(entry) => {
                let job_dir = self
                    .layout
                    .ensure_jobs_dir(catalog_id, job.task_id)
                    .await
                    .map_err(|e| {
                        error!("Failed to create jobs directory: {}", e);
                        JobStoreError::StorageError(format!(
                            "Failed to create jobs directory: {}",
                            e
                        ))
                    })?;
                debug!("Ensured jobs directory exists: {:?}", job_dir);

                self.save_job_to_file(&job, catalog_id).await?;

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

        let jobs = sorted_jobs(self.jobs.iter().map(|e| e.value().clone()));

        info!("Listed {} jobs", jobs.len());
        Ok(jobs)
    }

    async fn list_by_task(&self, task_id: Uuid) -> JobStoreResult<Vec<Job>> {
        debug!("Listing jobs for task: {}", task_id);

        let jobs = sorted_jobs(
            self.jobs
                .iter()
                .filter(|e| e.value().task_id == task_id)
                .map(|e| e.value().clone()),
        );

        info!("Listed {} jobs for task {}", jobs.len(), task_id);
        Ok(jobs)
    }

    async fn update(&self, job: Job) -> JobStoreResult<Job> {
        let job_id = job.job_id;

        debug!("Updating job: {}", job_id);

        let catalog_id = self.resolve_catalog_id(job.task_id).await?;

        match self.jobs.get_mut(&job_id) {
            Some(mut entry) => {
                self.save_job_to_file(&job, catalog_id).await?;

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

        let task_id = match self.jobs.get(&job_id) {
            Some(entry) => entry.value().task_id,
            None => return Err(JobStoreError::NotFound(job_id)),
        };

        let catalog_id = self.resolve_catalog_id(task_id).await?;

        let (_, job) = match self.jobs.remove(&job_id) {
            Some(entry) => entry,
            None => return Err(JobStoreError::NotFound(job_id)),
        };

        let job_path = self
            .layout
            .job_file_path(catalog_id, job.task_id, job.job_id);
        match tokio::fs::remove_file(&job_path).await {
            Ok(()) => debug!("Removed job file: {:?}", job_path),
            Err(e) if e.kind() == ErrorKind::NotFound => {}
            Err(e) => {
                error!("Failed to remove job file {:?}: {}", job_path, e);
                self.jobs.insert(job_id, job);
                return Err(JobStoreError::StorageError(format!(
                    "Failed to remove job file: {}",
                    e
                )));
            }
        }

        info!(
            "Job deleted successfully: {} (task: {}, status: {})",
            job_id, job.task_id, job.status
        );
        Ok(())
    }

    async fn delete_by_task(&self, task_id: Uuid) -> JobStoreResult<usize> {
        debug!("Deleting all jobs for task: {}", task_id);

        let job_ids: Vec<Uuid> = self
            .jobs
            .iter()
            .filter(|entry| entry.value().task_id == task_id)
            .map(|entry| *entry.key())
            .collect();

        let count = job_ids.len();
        if count == 0 {
            return Ok(0);
        }

        let catalog_id = self.resolve_catalog_id(task_id).await?;

        for job_id in job_ids {
            if let Some((_, job)) = self.jobs.remove(&job_id) {
                let job_path = self
                    .layout
                    .job_file_path(catalog_id, job.task_id, job.job_id);
                match tokio::fs::remove_file(&job_path).await {
                    Ok(()) => debug!("Removed job file: {:?}", job_path),
                    Err(e) if e.kind() == ErrorKind::NotFound => {}
                    Err(e) => error!("Failed to remove job file {:?}: {}", job_path, e),
                }
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{JobStatus, Task};
    use crate::storage::FileSystemTaskStore;
    use chrono::{DateTime, Duration, Utc};
    use tempfile::TempDir;
    use uuid::Uuid;

    struct TestStore {
        store: FileSystemJobStore,
        task_store: Arc<dyn TaskStore>,
        _temp_dir: TempDir,
    }

    async fn create_store() -> TestStore {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let storage_path = temp_dir.path().to_path_buf();
        let task_store: Arc<dyn TaskStore> =
            Arc::new(FileSystemTaskStore::new(storage_path.clone()).await);
        let store = FileSystemJobStore::new(storage_path, task_store.clone()).await;
        TestStore {
            store,
            task_store,
            _temp_dir: temp_dir,
        }
    }

    /// Creates a task in the task store so that resolve_catalog_id works.
    /// Returns the catalog_id assigned to the task.
    async fn setup_task(ts: &TestStore, task_id: Uuid) -> Uuid {
        let catalog_id = Uuid::new_v4();
        let task = Task {
            task_id,
            catalog_id,
            name: "test".to_string(),
            context: "test context".to_string(),
            created_at: Utc::now(),
        };
        ts.task_store.create(task).await.unwrap();
        catalog_id
    }

    fn create_test_job(task_id: Uuid, model: &str, photo_ids: Vec<Uuid>) -> Job {
        Job::new(task_id, model.to_string(), None, photo_ids)
    }

    fn create_test_job_with_timestamp(
        task_id: Uuid,
        model: &str,
        photo_ids: Vec<Uuid>,
        timestamp: DateTime<Utc>,
    ) -> Job {
        let job = Job::new(task_id, model.to_string(), None, photo_ids);
        let mut job_value = serde_json::to_value(&job).unwrap();
        job_value["created_at"] = serde_json::to_value(timestamp).unwrap();
        serde_json::from_value(job_value).unwrap()
    }

    #[tokio::test]
    async fn test_create_job() {
        let ts = create_store().await;
        let task_id = Uuid::new_v4();
        let catalog_id = setup_task(&ts, task_id).await;
        let photo_ids = vec![Uuid::new_v4(), Uuid::new_v4()];
        let job = create_test_job(task_id, "qwen3-vl:8b", photo_ids);

        let result = ts.store.create(job.clone()).await;

        assert!(result.is_ok());
        let created = result.unwrap();
        assert_eq!(created.job_id, job.job_id);
        assert_eq!(created.task_id, job.task_id);
        assert_eq!(created.model, job.model);
        assert_eq!(created.status, JobStatus::Queued);

        let exists = ts.store.exists(job.job_id).await.unwrap();
        assert!(exists);

        assert!(ts.store.layout.jobs_dir(catalog_id, task_id).exists());

        assert!(
            ts.store
                .layout
                .job_file_path(catalog_id, job.task_id, job.job_id)
                .exists()
        );
    }

    #[tokio::test]
    async fn test_create_duplicate_fails() {
        let ts = create_store().await;
        let task_id = Uuid::new_v4();
        setup_task(&ts, task_id).await;
        let photo_ids = vec![Uuid::new_v4()];
        let job = create_test_job(task_id, "qwen3-vl:8b", photo_ids);

        ts.store.create(job.clone()).await.unwrap();

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
        setup_task(&ts, task_id).await;
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
        setup_task(&ts, task_id).await;

        let now = Utc::now();
        let job1 =
            create_test_job_with_timestamp(task_id, "qwen3-vl:8b", vec![Uuid::new_v4()], now);
        let job2 = create_test_job_with_timestamp(
            task_id,
            "llava",
            vec![Uuid::new_v4()],
            now + Duration::seconds(1),
        );
        let job3 = create_test_job_with_timestamp(
            task_id,
            "qwen3-vl:8b",
            vec![Uuid::new_v4()],
            now + Duration::seconds(2),
        );

        ts.store.create(job2.clone()).await.unwrap();
        ts.store.create(job1.clone()).await.unwrap();
        ts.store.create(job3.clone()).await.unwrap();

        let jobs = ts.store.list().await.unwrap();

        assert_eq!(jobs.len(), 3);
        assert_eq!(jobs[0].job_id, job1.job_id);
        assert_eq!(jobs[1].job_id, job2.job_id);
        assert_eq!(jobs[2].job_id, job3.job_id);
    }

    #[tokio::test]
    async fn test_list_by_task() {
        let ts = create_store().await;

        let task_a = Uuid::new_v4();
        let task_b = Uuid::new_v4();
        let task_c = Uuid::new_v4();
        setup_task(&ts, task_a).await;
        setup_task(&ts, task_b).await;
        let photo_id = Uuid::new_v4();
        let job1 = create_test_job(task_a, "qwen3-vl:8b", vec![photo_id]);
        let job2 = create_test_job(task_a, "llava", vec![photo_id]);
        let job3 = create_test_job(task_b, "qwen3-vl:8b", vec![Uuid::new_v4()]);

        ts.store.create(job1).await.unwrap();
        ts.store.create(job2).await.unwrap();
        ts.store.create(job3).await.unwrap();

        let jobs_a = ts.store.list_by_task(task_a).await.unwrap();
        assert_eq!(jobs_a.len(), 2);
        assert!(jobs_a.iter().all(|j| j.task_id == task_a));

        let jobs_b = ts.store.list_by_task(task_b).await.unwrap();
        assert_eq!(jobs_b.len(), 1);
        assert_eq!(jobs_b[0].task_id, task_b);

        let jobs_c = ts.store.list_by_task(task_c).await.unwrap();
        assert!(jobs_c.is_empty());
    }

    #[tokio::test]
    async fn test_update_job() {
        let ts = create_store().await;
        let task_id = Uuid::new_v4();
        setup_task(&ts, task_id).await;
        let mut job = create_test_job(task_id, "qwen3-vl:8b", vec![Uuid::new_v4()]);

        ts.store.create(job.clone()).await.unwrap();

        job.start();
        let result = ts.store.update(job.clone()).await;

        assert!(result.is_ok());
        let updated = result.unwrap();
        assert_eq!(updated.status, JobStatus::Processing);
        assert!(updated.started_at.is_some());

        let retrieved = ts.store.get(job.job_id).await.unwrap().unwrap();
        assert_eq!(retrieved.status, JobStatus::Processing);
    }

    #[tokio::test]
    async fn test_update_nonexistent_fails() {
        let ts = create_store().await;
        let task_id = Uuid::new_v4();
        setup_task(&ts, task_id).await;
        let job = create_test_job(task_id, "qwen3-vl:8b", vec![Uuid::new_v4()]);

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
        let catalog_id = setup_task(&ts, task_id).await;
        let job = create_test_job(task_id, "qwen3-vl:8b", vec![Uuid::new_v4()]);

        ts.store.create(job.clone()).await.unwrap();
        let job_path = ts
            .store
            .layout
            .job_file_path(catalog_id, job.task_id, job.job_id);
        assert!(job_path.exists());

        let result = ts.store.delete(job.job_id).await;

        assert!(result.is_ok());

        let retrieved = ts.store.get(job.job_id).await.unwrap();
        assert!(retrieved.is_none());

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

        let task_a = Uuid::new_v4();
        let task_b = Uuid::new_v4();
        let catalog_id_a = setup_task(&ts, task_a).await;
        setup_task(&ts, task_b).await;
        let photo_id = Uuid::new_v4();
        let job1 = create_test_job(task_a, "qwen3-vl:8b", vec![photo_id]);
        let job2 = create_test_job(task_a, "llava", vec![photo_id]);
        let job3 = create_test_job(task_b, "qwen3-vl:8b", vec![Uuid::new_v4()]);

        ts.store.create(job1.clone()).await.unwrap();
        ts.store.create(job2.clone()).await.unwrap();
        ts.store.create(job3).await.unwrap();

        let job1_path = ts
            .store
            .layout
            .job_file_path(catalog_id_a, job1.task_id, job1.job_id);
        let job2_path = ts
            .store
            .layout
            .job_file_path(catalog_id_a, job2.task_id, job2.job_id);
        assert!(job1_path.exists());
        assert!(job2_path.exists());

        let deleted = ts.store.delete_by_task(task_a).await.unwrap();
        assert_eq!(deleted, 2);

        let jobs_a = ts.store.list_by_task(task_a).await.unwrap();
        assert!(jobs_a.is_empty());

        assert!(!job1_path.exists());
        assert!(!job2_path.exists());

        let jobs_b = ts.store.list_by_task(task_b).await.unwrap();
        assert_eq!(jobs_b.len(), 1);
    }

    #[tokio::test]
    async fn test_delete_by_task_nonexistent_returns_zero() {
        let ts = create_store().await;

        let result = ts.store.delete_by_task(Uuid::new_v4()).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_exists() {
        let ts = create_store().await;
        let task_id = Uuid::new_v4();
        setup_task(&ts, task_id).await;
        let job = create_test_job(task_id, "qwen3-vl:8b", vec![Uuid::new_v4()]);

        let exists = ts.store.exists(job.job_id).await.unwrap();
        assert!(!exists);

        ts.store.create(job.clone()).await.unwrap();

        let exists = ts.store.exists(job.job_id).await.unwrap();
        assert!(exists);

        let exists = ts.store.exists(Uuid::new_v4()).await.unwrap();
        assert!(!exists);
    }

    #[tokio::test]
    async fn test_count_by_task() {
        let ts = create_store().await;

        let task_a = Uuid::new_v4();
        let task_b = Uuid::new_v4();
        setup_task(&ts, task_a).await;
        setup_task(&ts, task_b).await;

        let count = ts.store.count_by_task(task_a).await.unwrap();
        assert_eq!(count, 0);

        let photo_id = Uuid::new_v4();
        let job1 = create_test_job(task_a, "qwen3-vl:8b", vec![photo_id]);
        let job2 = create_test_job(task_a, "llava", vec![photo_id]);
        let job3 = create_test_job(task_b, "qwen3-vl:8b", vec![Uuid::new_v4()]);

        ts.store.create(job1).await.unwrap();
        ts.store.create(job2).await.unwrap();
        ts.store.create(job3).await.unwrap();

        let count = ts.store.count_by_task(task_a).await.unwrap();
        assert_eq!(count, 2);

        let count = ts.store.count_by_task(task_b).await.unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn test_job_lifecycle_through_store() {
        let ts = create_store().await;
        let task_id = Uuid::new_v4();
        setup_task(&ts, task_id).await;
        let mut job = create_test_job(task_id, "qwen3-vl:8b", vec![Uuid::new_v4(), Uuid::new_v4()]);

        ts.store.create(job.clone()).await.unwrap();
        let retrieved = ts.store.get(job.job_id).await.unwrap().unwrap();
        assert_eq!(retrieved.status, JobStatus::Queued);

        job.start();
        ts.store.update(job.clone()).await.unwrap();
        let retrieved = ts.store.get(job.job_id).await.unwrap().unwrap();
        assert_eq!(retrieved.status, JobStatus::Processing);
        assert!(retrieved.started_at.is_some());

        job.complete();
        ts.store.update(job.clone()).await.unwrap();
        let retrieved = ts.store.get(job.job_id).await.unwrap().unwrap();
        assert_eq!(retrieved.status, JobStatus::Completed);
        assert!(retrieved.completed_at.is_some());
        assert!(retrieved.is_finished());
    }

    #[tokio::test]
    async fn test_persistence_survives_reload() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let storage_path = temp_dir.path().to_path_buf();

        let task_id = Uuid::new_v4();
        let catalog_id = Uuid::new_v4();
        let job1 = create_test_job(task_id, "qwen3-vl:8b", vec![Uuid::new_v4()]);
        let job2 = create_test_job(task_id, "llava", vec![Uuid::new_v4()]);
        let job1_id = job1.job_id;
        let job2_id = job2.job_id;

        {
            let task_store: Arc<dyn TaskStore> =
                Arc::new(FileSystemTaskStore::new(storage_path.clone()).await);
            let task = Task {
                task_id,
                catalog_id,
                name: "test".to_string(),
                context: "test".to_string(),
                created_at: Utc::now(),
            };
            task_store.create(task).await.unwrap();
            let store = FileSystemJobStore::new(storage_path.clone(), task_store).await;
            store.create(job1).await.unwrap();
            store.create(job2).await.unwrap();
            assert_eq!(store.count_by_task(task_id).await.unwrap(), 2);
        }

        let task_store: Arc<dyn TaskStore> =
            Arc::new(FileSystemTaskStore::new(storage_path.clone()).await);
        let store = FileSystemJobStore::new(storage_path, task_store).await;

        assert_eq!(store.count_by_task(task_id).await.unwrap(), 2);
        assert!(store.exists(job1_id).await.unwrap());
        assert!(store.exists(job2_id).await.unwrap());

        let loaded_job1 = store.get(job1_id).await.unwrap().unwrap();
        assert_eq!(loaded_job1.model, "qwen3-vl:8b");
    }

    #[tokio::test]
    async fn test_update_persists() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let storage_path = temp_dir.path().to_path_buf();

        let task_id = Uuid::new_v4();
        let catalog_id = Uuid::new_v4();
        let mut job = create_test_job(task_id, "qwen3-vl:8b", vec![Uuid::new_v4()]);
        let job_id = job.job_id;

        {
            let task_store: Arc<dyn TaskStore> =
                Arc::new(FileSystemTaskStore::new(storage_path.clone()).await);
            let task = Task {
                task_id,
                catalog_id,
                name: "test".to_string(),
                context: "test".to_string(),
                created_at: Utc::now(),
            };
            task_store.create(task).await.unwrap();
            let store = FileSystemJobStore::new(storage_path.clone(), task_store).await;
            store.create(job.clone()).await.unwrap();

            job.start();
            store.update(job.clone()).await.unwrap();
        }

        let task_store: Arc<dyn TaskStore> =
            Arc::new(FileSystemTaskStore::new(storage_path.clone()).await);
        let store = FileSystemJobStore::new(storage_path, task_store).await;
        let loaded = store.get(job_id).await.unwrap().unwrap();
        assert_eq!(loaded.status, JobStatus::Processing);
        assert!(loaded.started_at.is_some());
    }

    #[tokio::test]
    async fn test_delete_removes_from_filesystem() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let storage_path = temp_dir.path().to_path_buf();

        let task_id = Uuid::new_v4();
        let catalog_id = Uuid::new_v4();
        let job = create_test_job(task_id, "qwen3-vl:8b", vec![Uuid::new_v4()]);
        let job_id = job.job_id;

        {
            let task_store: Arc<dyn TaskStore> =
                Arc::new(FileSystemTaskStore::new(storage_path.clone()).await);
            let task = Task {
                task_id,
                catalog_id,
                name: "test".to_string(),
                context: "test".to_string(),
                created_at: Utc::now(),
            };
            task_store.create(task).await.unwrap();
            let store = FileSystemJobStore::new(storage_path.clone(), task_store).await;
            store.create(job).await.unwrap();
            store.delete(job_id).await.unwrap();
        }

        let task_store: Arc<dyn TaskStore> =
            Arc::new(FileSystemTaskStore::new(storage_path.clone()).await);
        let store = FileSystemJobStore::new(storage_path, task_store).await;
        assert_eq!(store.count_by_task(task_id).await.unwrap(), 0);
        assert!(!store.exists(job_id).await.unwrap());
    }

    /// Simulates a failure writing the `.tmp` file by making the jobs directory
    /// non-writable, then verifies the in-memory job is unchanged.
    #[cfg(unix)]
    #[tokio::test]
    async fn test_update_memory_unchanged_when_tmp_write_fails() {
        use std::os::unix::fs::PermissionsExt;

        let ts = create_store().await;
        let task_id = Uuid::new_v4();
        let catalog_id = setup_task(&ts, task_id).await;
        let job = create_test_job(task_id, "qwen3-vl:8b", vec![Uuid::new_v4()]);
        ts.store.create(job.clone()).await.unwrap();

        let jobs_dir = ts.store.layout.jobs_dir(catalog_id, task_id);
        std::fs::set_permissions(&jobs_dir, std::fs::Permissions::from_mode(0o555)).unwrap();

        let mut updated = job.clone();
        updated.start();
        let result = ts.store.update(updated).await;

        std::fs::set_permissions(&jobs_dir, std::fs::Permissions::from_mode(0o755)).unwrap();

        assert!(result.is_err());
        let in_memory = ts.store.get(job.job_id).await.unwrap().unwrap();
        assert_eq!(in_memory.status, job.status);
    }

    /// Simulates a rename failure by replacing the job JSON file with a directory,
    /// then verifies the in-memory job is unchanged.
    #[cfg(unix)]
    #[tokio::test]
    async fn test_update_memory_unchanged_when_rename_fails() {
        let ts = create_store().await;
        let task_id = Uuid::new_v4();
        let catalog_id = setup_task(&ts, task_id).await;
        let job = create_test_job(task_id, "qwen3-vl:8b", vec![Uuid::new_v4()]);
        ts.store.create(job.clone()).await.unwrap();

        let job_path = ts
            .store
            .layout
            .job_file_path(catalog_id, task_id, job.job_id);
        tokio::fs::remove_file(&job_path).await.unwrap();
        tokio::fs::create_dir(&job_path).await.unwrap();

        let mut updated = job.clone();
        updated.start();
        let result = ts.store.update(updated).await;

        tokio::fs::remove_dir(&job_path).await.unwrap();

        assert!(result.is_err());
        let in_memory = ts.store.get(job.job_id).await.unwrap().unwrap();
        assert_eq!(in_memory.status, job.status);
    }

    /// Simulates a filesystem failure during delete by making the jobs directory
    /// non-writable, then verifies the job is rolled back into memory.
    #[cfg(unix)]
    #[tokio::test]
    async fn test_delete_memory_rolled_back_when_filesystem_fails() {
        use std::os::unix::fs::PermissionsExt;

        let ts = create_store().await;
        let task_id = Uuid::new_v4();
        let catalog_id = setup_task(&ts, task_id).await;
        let job = create_test_job(task_id, "qwen3-vl:8b", vec![Uuid::new_v4()]);
        ts.store.create(job.clone()).await.unwrap();

        let jobs_dir = ts.store.layout.jobs_dir(catalog_id, task_id);
        std::fs::set_permissions(&jobs_dir, std::fs::Permissions::from_mode(0o555)).unwrap();

        let result = ts.store.delete(job.job_id).await;

        std::fs::set_permissions(&jobs_dir, std::fs::Permissions::from_mode(0o755)).unwrap();

        assert!(result.is_err());
        let in_memory = ts.store.get(job.job_id).await.unwrap();
        assert!(in_memory.is_some());
    }

    #[tokio::test]
    async fn test_multiple_tasks_with_jobs_load_correctly() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let storage_path = temp_dir.path().to_path_buf();

        let task_a = Uuid::new_v4();
        let task_b = Uuid::new_v4();
        let catalog_id = Uuid::new_v4();
        let job1 = create_test_job(task_a, "qwen3-vl:8b", vec![Uuid::new_v4()]);
        let job2 = create_test_job(task_a, "llava", vec![Uuid::new_v4()]);
        let job3 = create_test_job(task_b, "qwen3-vl:8b", vec![Uuid::new_v4()]);
        let job4 = create_test_job(task_b, "llava", vec![Uuid::new_v4()]);

        {
            let task_store: Arc<dyn TaskStore> =
                Arc::new(FileSystemTaskStore::new(storage_path.clone()).await);
            for &tid in &[task_a, task_b] {
                let task = Task {
                    task_id: tid,
                    catalog_id,
                    name: "test".to_string(),
                    context: "test".to_string(),
                    created_at: Utc::now(),
                };
                task_store.create(task).await.unwrap();
            }
            let store = FileSystemJobStore::new(storage_path.clone(), task_store).await;
            store.create(job1).await.unwrap();
            store.create(job2).await.unwrap();
            store.create(job3).await.unwrap();
            store.create(job4).await.unwrap();
        }

        let task_store: Arc<dyn TaskStore> =
            Arc::new(FileSystemTaskStore::new(storage_path.clone()).await);
        let store = FileSystemJobStore::new(storage_path, task_store).await;
        assert_eq!(store.list().await.unwrap().len(), 4);
        assert_eq!(store.count_by_task(task_a).await.unwrap(), 2);
        assert_eq!(store.count_by_task(task_b).await.unwrap(), 2);
    }

    /// Sets up a task on disk and returns its catalog_id and the job file path for `job`.
    async fn setup_task_and_job(
        storage_path: &PathBuf,
        task_id: Uuid,
        catalog_id: Uuid,
        job: &Job,
    ) -> PathBuf {
        let task_store: Arc<dyn TaskStore> =
            Arc::new(FileSystemTaskStore::new(storage_path.clone()).await);
        let task = Task {
            task_id,
            catalog_id,
            name: "test".to_string(),
            context: "ctx".to_string(),
            created_at: Utc::now(),
        };
        task_store.create(task).await.unwrap();
        let store = FileSystemJobStore::new(storage_path.clone(), task_store).await;
        store.create(job.clone()).await.unwrap();
        store.layout.job_file_path(catalog_id, task_id, job.job_id)
    }

    #[tokio::test]
    async fn test_load_quarantines_corrupt_job_file() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let storage_path = temp_dir.path().to_path_buf();
        let task_id = Uuid::new_v4();
        let catalog_id = Uuid::new_v4();
        let job = create_test_job(task_id, "llava", vec![Uuid::new_v4()]);
        let job_id = job.job_id;

        let job_file = setup_task_and_job(&storage_path, task_id, catalog_id, &job).await;
        tokio::fs::write(&job_file, b"not valid json")
            .await
            .unwrap();

        let task_store: Arc<dyn TaskStore> =
            Arc::new(FileSystemTaskStore::new(storage_path.clone()).await);
        let store = FileSystemJobStore::new(storage_path, task_store).await;

        assert!(!store.exists(job_id).await.unwrap());
        assert!(!job_file.exists());
        assert!(store.layout.quarantine_path_for(&job_file).exists());
    }

    #[tokio::test]
    async fn test_load_quarantines_inconsistent_job_id() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let storage_path = temp_dir.path().to_path_buf();
        let task_id = Uuid::new_v4();
        let catalog_id = Uuid::new_v4();
        let job = create_test_job(task_id, "llava", vec![Uuid::new_v4()]);

        let job_file = setup_task_and_job(&storage_path, task_id, catalog_id, &job).await;

        let mut tampered = job.clone();
        tampered.job_id = Uuid::new_v4();
        tokio::fs::write(&job_file, serde_json::to_string(&tampered).unwrap())
            .await
            .unwrap();

        let task_store: Arc<dyn TaskStore> =
            Arc::new(FileSystemTaskStore::new(storage_path.clone()).await);
        let store = FileSystemJobStore::new(storage_path, task_store).await;

        assert_eq!(store.count_by_task(task_id).await.unwrap(), 0);
        assert!(!job_file.exists());
        assert!(store.layout.quarantine_path_for(&job_file).exists());
    }

    #[tokio::test]
    async fn test_load_quarantines_inconsistent_task_id() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let storage_path = temp_dir.path().to_path_buf();
        let task_id = Uuid::new_v4();
        let catalog_id = Uuid::new_v4();
        let job = create_test_job(task_id, "llava", vec![Uuid::new_v4()]);

        let job_file = setup_task_and_job(&storage_path, task_id, catalog_id, &job).await;

        let mut tampered = job.clone();
        tampered.task_id = Uuid::new_v4();
        tokio::fs::write(&job_file, serde_json::to_string(&tampered).unwrap())
            .await
            .unwrap();

        let task_store: Arc<dyn TaskStore> =
            Arc::new(FileSystemTaskStore::new(storage_path.clone()).await);
        let store = FileSystemJobStore::new(storage_path, task_store).await;

        assert_eq!(store.count_by_task(task_id).await.unwrap(), 0);
        assert!(!job_file.exists());
        assert!(store.layout.quarantine_path_for(&job_file).exists());
    }

    #[tokio::test]
    async fn test_load_valid_job_not_quarantined() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let storage_path = temp_dir.path().to_path_buf();
        let task_id = Uuid::new_v4();
        let catalog_id = Uuid::new_v4();
        let job = create_test_job(task_id, "llava", vec![Uuid::new_v4()]);
        let job_id = job.job_id;

        setup_task_and_job(&storage_path, task_id, catalog_id, &job).await;

        let task_store: Arc<dyn TaskStore> =
            Arc::new(FileSystemTaskStore::new(storage_path.clone()).await);
        let store = FileSystemJobStore::new(storage_path, task_store).await;

        assert!(store.exists(job_id).await.unwrap());
        assert!(!store.layout.quarantine_dir().exists());
    }
}
