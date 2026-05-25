// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 The Photometoria contributors

//! Filesystem-backed implementation of ActivityStore
//!
//! This module provides a thread-safe implementation of the ActivityStore trait
//! with full persistence to the filesystem. Activity metadata is stored as JSON files
//! within each project's directory.

use async_trait::async_trait;
use chrono::Local;
use dashmap::DashMap;
use dashmap::mapref::entry::Entry;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::models::Activity;

use super::utils::{
    load_json_from_file, parse_uuid_from_dir, try_quarantine, try_scan_project_dirs,
};
use super::{
    ActivityStore, ActivityStoreError, ActivityStoreResult, FileSystemLayout, ProjectStore,
};

fn sorted_activities(iter: impl Iterator<Item = Activity>) -> Vec<Activity> {
    let mut activities: Vec<Activity> = iter.collect();
    activities.sort_by_key(|a| a.created_at);
    activities
}

/// Filesystem-backed implementation of ActivityStore with full persistence.
///
/// This implementation stores activity metadata both in memory (using DashMap for
/// fast concurrent access) and on the filesystem (as JSON files for persistence).
///
/// ## Characteristics
///
/// - **Thread-safe**: Supports concurrent access from multiple Tokio tasks
/// - **Lock-free reads**: Get operations don't acquire locks
/// - **Fine-grained locking**: Writes lock only the specific shard
/// - **Full persistence**: Activity metadata survives server restarts
/// - **Nested structure**: Activities stored in project-specific subdirectories
///
/// ## Persistence Strategy
///
/// - Metadata is written to disk after each create/update operation
/// - On startup, all existing activity JSON files are loaded into memory
/// - Deleting an activity removes both the in-memory entry and the JSON file
/// - Deleting a project automatically removes all activity files in the project's jobs/ directory
///
/// For details on the filesystem layout, see [`FileSystemLayout`]
pub struct FileSystemActivityStore {
    activities: Arc<DashMap<Uuid, Activity>>,
    layout: FileSystemLayout,
    project_store: Arc<dyn ProjectStore>,
}

impl FileSystemActivityStore {
    /// Creates a new filesystem-backed activity store.
    ///
    /// This constructor loads all existing activities from the filesystem.
    /// Any errors during loading are logged but don't prevent startup.
    ///
    /// # Arguments
    /// * `storage_path` - Base path for storing project directories
    /// * `project_store` - Reference to the project store for resolving project-to-catalog relationships
    pub async fn new(storage_path: PathBuf, project_store: Arc<dyn ProjectStore>) -> Self {
        Self::new_with_boot_ts(
            storage_path,
            project_store,
            Local::now().format("%Y%m%d_%H%M%S").to_string(),
        )
        .await
    }

    /// Creates a new filesystem-backed activity store with an explicit boot timestamp.
    ///
    /// Use this when multiple stores must share the same quarantine directory for
    /// a single server boot.
    pub async fn new_with_boot_ts(
        storage_path: PathBuf,
        project_store: Arc<dyn ProjectStore>,
        boot_ts: String,
    ) -> Self {
        let store = Self {
            activities: Arc::new(DashMap::new()),
            layout: FileSystemLayout::new_with_boot_ts(storage_path, boot_ts),
            project_store,
        };
        store.load_all().await;
        store
    }

    /// Resolves the catalog identity for a given project.
    async fn resolve_catalog_id(&self, project_id: Uuid) -> ActivityStoreResult<Uuid> {
        let project = self
            .project_store
            .get(project_id)
            .await
            .map_err(|e| {
                ActivityStoreError::StorageError(format!("Failed to query project store: {}", e))
            })?
            .ok_or_else(|| {
                ActivityStoreError::StorageError(format!(
                    "Cannot resolve catalog: project {} not found",
                    project_id
                ))
            })?;
        Ok(project.catalog_id)
    }

    /// Loads all activities from the filesystem into memory.
    ///
    /// Scans the catalog hierarchy: `catalogs/{catalog_id}/tasks/{project_id}/jobs/*.json`
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
            let (n_loaded, n_errors) = self.load_catalog_activities(catalog_id).await;
            loaded += n_loaded;
            errors += n_errors;
        }

        info!(
            "Loaded {} activities from filesystem ({} errors)",
            loaded, errors
        );
    }

    /// Loads all activities for a single catalog from the filesystem.
    ///
    /// Returns the count of successfully loaded activities and errors encountered.
    /// Activity files with corrupt, inconsistent, or unresolvable data are moved to
    /// the boot-scoped quarantine directory.
    async fn load_catalog_activities(&self, catalog_id: Uuid) -> (usize, usize) {
        let Some(project_dirs) = try_scan_project_dirs(&self.layout, catalog_id).await else {
            return (0, 1);
        };

        let mut loaded = 0;
        let mut errors = 0;

        for project_dir in project_dirs {
            let Some(project_id) = parse_uuid_from_dir(&project_dir) else {
                warn!("Skipping invalid project directory: {:?}", project_dir);
                continue;
            };

            let activity_files = match self.layout.scan_job_files(catalog_id, project_id).await {
                Ok(files) => files,
                Err(e) => {
                    warn!(
                        "Failed to scan activities for project {}: {}",
                        project_id, e
                    );
                    errors += 1;
                    continue;
                }
            };

            for activity_path in activity_files {
                let Some(filename_id) = activity_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .and_then(|s| s.parse::<Uuid>().ok())
                else {
                    warn!("Skipping non-UUID activity file: {:?}", activity_path);
                    continue;
                };

                let activity = match load_json_from_file::<Activity>(&activity_path).await {
                    Ok(a) => a,
                    Err(e) => {
                        error!(
                            "Corrupt activity file {:?}: {} — quarantining",
                            activity_path, e
                        );
                        try_quarantine(&self.layout, &activity_path).await;
                        errors += 1;
                        continue;
                    }
                };

                if filename_id != activity.activity_id {
                    error!(
                        "Inconsistent activity file {:?}: activity_id {} does not match filename {} — quarantining",
                        activity_path, activity.activity_id, filename_id
                    );
                    try_quarantine(&self.layout, &activity_path).await;
                    errors += 1;
                    continue;
                }

                if activity.project_id != project_id {
                    error!(
                        "Inconsistent activity file {:?}: project_id {} does not match directory {} — quarantining",
                        activity_path, activity.project_id, project_id
                    );
                    try_quarantine(&self.layout, &activity_path).await;
                    errors += 1;
                    continue;
                }

                self.activities.insert(activity.activity_id, activity);
                loaded += 1;
            }
        }

        (loaded, errors)
    }

    /// Saves an activity's metadata to the filesystem atomically.
    ///
    /// Writes to a temporary file first, then renames to the final path.
    /// This ensures the target file is never left in a partially-written state.
    async fn save_activity_to_file(
        &self,
        activity: &Activity,
        catalog_id: Uuid,
    ) -> ActivityStoreResult<()> {
        let path = self
            .layout
            .job_file_path(catalog_id, activity.project_id, activity.activity_id);
        let tmp_path = path.with_extension("tmp");
        let content = serde_json::to_string_pretty(activity).map_err(|e| {
            ActivityStoreError::StorageError(format!("Failed to serialize activity: {}", e))
        })?;

        tokio::fs::write(&tmp_path, &content).await.map_err(|e| {
            error!(
                "Failed to write temporary activity file {:?}: {}",
                tmp_path, e
            );
            ActivityStoreError::StorageError(format!("Failed to write activity file: {}", e))
        })?;

        tokio::fs::rename(&tmp_path, &path).await.map_err(|e| {
            error!("Failed to rename {:?} -> {:?}: {}", tmp_path, path, e);
            ActivityStoreError::StorageError(format!(
                "Failed to atomically save activity file: {}",
                e
            ))
        })?;

        debug!("Saved activity metadata to {:?}", path);
        Ok(())
    }
}

#[async_trait]
impl ActivityStore for FileSystemActivityStore {
    async fn create(&self, activity: Activity) -> ActivityStoreResult<Activity> {
        let activity_id = activity.activity_id;

        debug!("Attempting to create activity: {}", activity_id);

        let catalog_id = self.resolve_catalog_id(activity.project_id).await?;

        match self.activities.entry(activity_id) {
            Entry::Occupied(_) => Err(ActivityStoreError::AlreadyExists(activity_id)),
            Entry::Vacant(entry) => {
                let jobs_dir = self
                    .layout
                    .ensure_jobs_dir(catalog_id, activity.project_id)
                    .await
                    .map_err(|e| {
                        error!("Failed to create jobs directory: {}", e);
                        ActivityStoreError::StorageError(format!(
                            "Failed to create jobs directory: {}",
                            e
                        ))
                    })?;
                debug!("Ensured jobs directory exists: {:?}", jobs_dir);

                self.save_activity_to_file(&activity, catalog_id).await?;

                entry.insert(activity.clone());
                info!(
                    "Activity created successfully: {} (project: {}, model: '{}', photos: {})",
                    activity_id,
                    activity.project_id,
                    activity.model,
                    activity.photo_count()
                );
                Ok(activity)
            }
        }
    }

    async fn get(&self, activity_id: Uuid) -> ActivityStoreResult<Option<Activity>> {
        debug!("Retrieving activity: {}", activity_id);

        Ok(self
            .activities
            .get(&activity_id)
            .map(|entry| entry.value().clone()))
    }

    async fn list(&self) -> ActivityStoreResult<Vec<Activity>> {
        debug!("Listing all activities");

        let activities = sorted_activities(self.activities.iter().map(|e| e.value().clone()));

        info!("Listed {} activities", activities.len());
        Ok(activities)
    }

    async fn list_by_task(&self, project_id: Uuid) -> ActivityStoreResult<Vec<Activity>> {
        debug!("Listing activities for project: {}", project_id);

        let activities = sorted_activities(
            self.activities
                .iter()
                .filter(|e| e.value().project_id == project_id)
                .map(|e| e.value().clone()),
        );

        info!(
            "Listed {} activities for project {}",
            activities.len(),
            project_id
        );
        Ok(activities)
    }

    async fn update(&self, activity: Activity) -> ActivityStoreResult<Activity> {
        let activity_id = activity.activity_id;

        debug!("Updating activity: {}", activity_id);

        let catalog_id = self.resolve_catalog_id(activity.project_id).await?;

        match self.activities.get_mut(&activity_id) {
            Some(mut entry) => {
                self.save_activity_to_file(&activity, catalog_id).await?;

                let old_status = entry.status;
                *entry = activity.clone();
                info!(
                    "Activity updated successfully: {} (status: {} -> {})",
                    activity_id, old_status, activity.status
                );
                Ok(activity)
            }
            None => Err(ActivityStoreError::NotFound(activity_id)),
        }
    }

    async fn delete(&self, activity_id: Uuid) -> ActivityStoreResult<()> {
        debug!("Deleting activity: {}", activity_id);

        let project_id = match self.activities.get(&activity_id) {
            Some(entry) => entry.value().project_id,
            None => return Err(ActivityStoreError::NotFound(activity_id)),
        };

        let catalog_id = self.resolve_catalog_id(project_id).await?;

        let (_, activity) = match self.activities.remove(&activity_id) {
            Some(entry) => entry,
            None => return Err(ActivityStoreError::NotFound(activity_id)),
        };

        let activity_path =
            self.layout
                .job_file_path(catalog_id, activity.project_id, activity.activity_id);
        match tokio::fs::remove_file(&activity_path).await {
            Ok(()) => debug!("Removed activity file: {:?}", activity_path),
            Err(e) if e.kind() == ErrorKind::NotFound => {}
            Err(e) => {
                error!("Failed to remove activity file {:?}: {}", activity_path, e);
                self.activities.insert(activity_id, activity);
                return Err(ActivityStoreError::StorageError(format!(
                    "Failed to remove activity file: {}",
                    e
                )));
            }
        }

        info!(
            "Activity deleted successfully: {} (project: {}, status: {})",
            activity_id, activity.project_id, activity.status
        );
        Ok(())
    }

    async fn delete_by_task(&self, project_id: Uuid) -> ActivityStoreResult<usize> {
        debug!("Deleting all activities for project: {}", project_id);

        let activity_ids: Vec<Uuid> = self
            .activities
            .iter()
            .filter(|entry| entry.value().project_id == project_id)
            .map(|entry| *entry.key())
            .collect();

        let count = activity_ids.len();
        if count == 0 {
            return Ok(0);
        }

        let catalog_id = self.resolve_catalog_id(project_id).await?;

        for activity_id in activity_ids {
            if let Some((_, activity)) = self.activities.remove(&activity_id) {
                let activity_path = self.layout.job_file_path(
                    catalog_id,
                    activity.project_id,
                    activity.activity_id,
                );
                match tokio::fs::remove_file(&activity_path).await {
                    Ok(()) => debug!("Removed activity file: {:?}", activity_path),
                    Err(e) if e.kind() == ErrorKind::NotFound => {}
                    Err(e) => error!("Failed to remove activity file {:?}: {}", activity_path, e),
                }
            }
        }

        info!("Deleted {} activities for project {}", count, project_id);
        Ok(count)
    }

    async fn exists(&self, activity_id: Uuid) -> ActivityStoreResult<bool> {
        debug!("Checking if activity exists: {}", activity_id);

        Ok(self.activities.contains_key(&activity_id))
    }

    async fn count_by_task(&self, project_id: Uuid) -> ActivityStoreResult<usize> {
        debug!("Counting activities for project: {}", project_id);

        let count = self
            .activities
            .iter()
            .filter(|entry| entry.value().project_id == project_id)
            .count();

        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Activity, ActivityStatus, Project};
    use crate::storage::FileSystemProjectStore;
    use chrono::{DateTime, Duration, Utc};
    use tempfile::TempDir;
    use uuid::Uuid;

    struct TestStore {
        store: FileSystemActivityStore,
        project_store: Arc<dyn ProjectStore>,
        _temp_dir: TempDir,
    }

    async fn create_store() -> TestStore {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let storage_path = temp_dir.path().to_path_buf();
        let project_store: Arc<dyn ProjectStore> =
            Arc::new(FileSystemProjectStore::new(storage_path.clone()).await);
        let store = FileSystemActivityStore::new(storage_path, project_store.clone()).await;
        TestStore {
            store,
            project_store,
            _temp_dir: temp_dir,
        }
    }

    /// Creates a project in the project store so that resolve_catalog_id works.
    /// Returns the catalog_id assigned to the project.
    async fn setup_project(ts: &TestStore, project_id: Uuid) -> Uuid {
        let catalog_id = Uuid::new_v4();
        let project = Project {
            project_id,
            catalog_id,
            name: "test".to_string(),
            context: "test context".to_string(),
            created_at: Utc::now(),
        };
        ts.project_store.create(project).await.unwrap();
        catalog_id
    }

    fn create_test_activity(project_id: Uuid, model: &str, photo_ids: Vec<Uuid>) -> Activity {
        Activity::new(
            project_id,
            "ollama".to_string(),
            model.to_string(),
            None,
            photo_ids,
        )
    }

    fn create_test_activity_with_timestamp(
        project_id: Uuid,
        model: &str,
        photo_ids: Vec<Uuid>,
        timestamp: DateTime<Utc>,
    ) -> Activity {
        let activity = Activity::new(
            project_id,
            "ollama".to_string(),
            model.to_string(),
            None,
            photo_ids,
        );
        let mut value = serde_json::to_value(&activity).unwrap();
        value["created_at"] = serde_json::to_value(timestamp).unwrap();
        serde_json::from_value(value).unwrap()
    }

    #[tokio::test]
    async fn test_create_activity() {
        let ts = create_store().await;
        let project_id = Uuid::new_v4();
        let catalog_id = setup_project(&ts, project_id).await;
        let photo_ids = vec![Uuid::new_v4(), Uuid::new_v4()];
        let activity = create_test_activity(project_id, "qwen3-vl:8b", photo_ids);

        let result = ts.store.create(activity.clone()).await;

        assert!(result.is_ok());
        let created = result.unwrap();
        assert_eq!(created.activity_id, activity.activity_id);
        assert_eq!(created.project_id, activity.project_id);
        assert_eq!(created.model, activity.model);
        assert_eq!(created.status, ActivityStatus::Queued);

        let exists = ts.store.exists(activity.activity_id).await.unwrap();
        assert!(exists);

        assert!(ts.store.layout.jobs_dir(catalog_id, project_id).exists());

        assert!(
            ts.store
                .layout
                .job_file_path(catalog_id, activity.project_id, activity.activity_id)
                .exists()
        );
    }

    #[tokio::test]
    async fn test_create_duplicate_fails() {
        let ts = create_store().await;
        let project_id = Uuid::new_v4();
        setup_project(&ts, project_id).await;
        let photo_ids = vec![Uuid::new_v4()];
        let activity = create_test_activity(project_id, "qwen3-vl:8b", photo_ids);

        ts.store.create(activity.clone()).await.unwrap();

        let result = ts.store.create(activity.clone()).await;

        assert!(result.is_err());
        match result {
            Err(ActivityStoreError::AlreadyExists(id)) => {
                assert_eq!(id, activity.activity_id);
            }
            _ => panic!("Expected AlreadyExists error"),
        }
    }

    #[tokio::test]
    async fn test_get_existing_activity() {
        let ts = create_store().await;
        let project_id = Uuid::new_v4();
        setup_project(&ts, project_id).await;
        let photo_ids = vec![Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()];
        let activity = create_test_activity(project_id, "llava", photo_ids);

        ts.store.create(activity.clone()).await.unwrap();

        let result = ts.store.get(activity.activity_id).await.unwrap();

        assert!(result.is_some());
        let retrieved = result.unwrap();
        assert_eq!(retrieved.activity_id, activity.activity_id);
        assert_eq!(retrieved.project_id, activity.project_id);
        assert_eq!(retrieved.model, activity.model);
        assert_eq!(retrieved.photo_count(), 3);
    }

    #[tokio::test]
    async fn test_get_nonexistent_activity() {
        let ts = create_store().await;

        let result = ts.store.get(Uuid::new_v4()).await.unwrap();

        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_list_activities_ordered_by_created_at() {
        let ts = create_store().await;
        let project_id = Uuid::new_v4();
        setup_project(&ts, project_id).await;

        let now = Utc::now();
        let activity1 = create_test_activity_with_timestamp(
            project_id,
            "qwen3-vl:8b",
            vec![Uuid::new_v4()],
            now,
        );
        let activity2 = create_test_activity_with_timestamp(
            project_id,
            "llava",
            vec![Uuid::new_v4()],
            now + Duration::seconds(1),
        );
        let activity3 = create_test_activity_with_timestamp(
            project_id,
            "qwen3-vl:8b",
            vec![Uuid::new_v4()],
            now + Duration::seconds(2),
        );

        ts.store.create(activity2.clone()).await.unwrap();
        ts.store.create(activity1.clone()).await.unwrap();
        ts.store.create(activity3.clone()).await.unwrap();

        let activities = ts.store.list().await.unwrap();

        assert_eq!(activities.len(), 3);
        assert_eq!(activities[0].activity_id, activity1.activity_id);
        assert_eq!(activities[1].activity_id, activity2.activity_id);
        assert_eq!(activities[2].activity_id, activity3.activity_id);
    }

    #[tokio::test]
    async fn test_list_by_task() {
        let ts = create_store().await;

        let project_a = Uuid::new_v4();
        let project_b = Uuid::new_v4();
        let project_c = Uuid::new_v4();
        setup_project(&ts, project_a).await;
        setup_project(&ts, project_b).await;
        let photo_id = Uuid::new_v4();
        let activity1 = create_test_activity(project_a, "qwen3-vl:8b", vec![photo_id]);
        let activity2 = create_test_activity(project_a, "llava", vec![photo_id]);
        let activity3 = create_test_activity(project_b, "qwen3-vl:8b", vec![Uuid::new_v4()]);

        ts.store.create(activity1).await.unwrap();
        ts.store.create(activity2).await.unwrap();
        ts.store.create(activity3).await.unwrap();

        let activities_a = ts.store.list_by_task(project_a).await.unwrap();
        assert_eq!(activities_a.len(), 2);
        assert!(activities_a.iter().all(|a| a.project_id == project_a));

        let activities_b = ts.store.list_by_task(project_b).await.unwrap();
        assert_eq!(activities_b.len(), 1);
        assert_eq!(activities_b[0].project_id, project_b);

        let activities_c = ts.store.list_by_task(project_c).await.unwrap();
        assert!(activities_c.is_empty());
    }

    #[tokio::test]
    async fn test_update_activity() {
        let ts = create_store().await;
        let project_id = Uuid::new_v4();
        setup_project(&ts, project_id).await;
        let mut activity = create_test_activity(project_id, "qwen3-vl:8b", vec![Uuid::new_v4()]);

        ts.store.create(activity.clone()).await.unwrap();

        activity.start();
        let result = ts.store.update(activity.clone()).await;

        assert!(result.is_ok());
        let updated = result.unwrap();
        assert_eq!(updated.status, ActivityStatus::Processing);
        assert!(updated.started_at.is_some());

        let retrieved = ts.store.get(activity.activity_id).await.unwrap().unwrap();
        assert_eq!(retrieved.status, ActivityStatus::Processing);
    }

    #[tokio::test]
    async fn test_update_nonexistent_fails() {
        let ts = create_store().await;
        let project_id = Uuid::new_v4();
        setup_project(&ts, project_id).await;
        let activity = create_test_activity(project_id, "qwen3-vl:8b", vec![Uuid::new_v4()]);

        let result = ts.store.update(activity.clone()).await;

        assert!(result.is_err());
        match result {
            Err(ActivityStoreError::NotFound(id)) => {
                assert_eq!(id, activity.activity_id);
            }
            _ => panic!("Expected NotFound error"),
        }
    }

    #[tokio::test]
    async fn test_delete_activity() {
        let ts = create_store().await;
        let project_id = Uuid::new_v4();
        let catalog_id = setup_project(&ts, project_id).await;
        let activity = create_test_activity(project_id, "qwen3-vl:8b", vec![Uuid::new_v4()]);

        ts.store.create(activity.clone()).await.unwrap();
        let activity_path =
            ts.store
                .layout
                .job_file_path(catalog_id, activity.project_id, activity.activity_id);
        assert!(activity_path.exists());

        let result = ts.store.delete(activity.activity_id).await;

        assert!(result.is_ok());

        let retrieved = ts.store.get(activity.activity_id).await.unwrap();
        assert!(retrieved.is_none());

        assert!(!activity_path.exists());
    }

    #[tokio::test]
    async fn test_delete_nonexistent_fails() {
        let ts = create_store().await;

        let nonexistent_id = Uuid::new_v4();
        let result = ts.store.delete(nonexistent_id).await;

        assert!(result.is_err());
        match result {
            Err(ActivityStoreError::NotFound(id)) => {
                assert_eq!(id, nonexistent_id);
            }
            _ => panic!("Expected NotFound error"),
        }
    }

    #[tokio::test]
    async fn test_delete_by_task() {
        let ts = create_store().await;

        let project_a = Uuid::new_v4();
        let project_b = Uuid::new_v4();
        let catalog_id_a = setup_project(&ts, project_a).await;
        setup_project(&ts, project_b).await;
        let photo_id = Uuid::new_v4();
        let activity1 = create_test_activity(project_a, "qwen3-vl:8b", vec![photo_id]);
        let activity2 = create_test_activity(project_a, "llava", vec![photo_id]);
        let activity3 = create_test_activity(project_b, "qwen3-vl:8b", vec![Uuid::new_v4()]);

        ts.store.create(activity1.clone()).await.unwrap();
        ts.store.create(activity2.clone()).await.unwrap();
        ts.store.create(activity3).await.unwrap();

        let path1 = ts.store.layout.job_file_path(
            catalog_id_a,
            activity1.project_id,
            activity1.activity_id,
        );
        let path2 = ts.store.layout.job_file_path(
            catalog_id_a,
            activity2.project_id,
            activity2.activity_id,
        );
        assert!(path1.exists());
        assert!(path2.exists());

        let deleted = ts.store.delete_by_task(project_a).await.unwrap();
        assert_eq!(deleted, 2);

        let activities_a = ts.store.list_by_task(project_a).await.unwrap();
        assert!(activities_a.is_empty());

        assert!(!path1.exists());
        assert!(!path2.exists());

        let activities_b = ts.store.list_by_task(project_b).await.unwrap();
        assert_eq!(activities_b.len(), 1);
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
        let project_id = Uuid::new_v4();
        setup_project(&ts, project_id).await;
        let activity = create_test_activity(project_id, "qwen3-vl:8b", vec![Uuid::new_v4()]);

        let exists = ts.store.exists(activity.activity_id).await.unwrap();
        assert!(!exists);

        ts.store.create(activity.clone()).await.unwrap();

        let exists = ts.store.exists(activity.activity_id).await.unwrap();
        assert!(exists);

        let exists = ts.store.exists(Uuid::new_v4()).await.unwrap();
        assert!(!exists);
    }

    #[tokio::test]
    async fn test_count_by_task() {
        let ts = create_store().await;

        let project_a = Uuid::new_v4();
        let project_b = Uuid::new_v4();
        setup_project(&ts, project_a).await;
        setup_project(&ts, project_b).await;

        let count = ts.store.count_by_task(project_a).await.unwrap();
        assert_eq!(count, 0);

        let photo_id = Uuid::new_v4();
        let activity1 = create_test_activity(project_a, "qwen3-vl:8b", vec![photo_id]);
        let activity2 = create_test_activity(project_a, "llava", vec![photo_id]);
        let activity3 = create_test_activity(project_b, "qwen3-vl:8b", vec![Uuid::new_v4()]);

        ts.store.create(activity1).await.unwrap();
        ts.store.create(activity2).await.unwrap();
        ts.store.create(activity3).await.unwrap();

        let count = ts.store.count_by_task(project_a).await.unwrap();
        assert_eq!(count, 2);

        let count = ts.store.count_by_task(project_b).await.unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn test_activity_lifecycle_through_store() {
        let ts = create_store().await;
        let project_id = Uuid::new_v4();
        setup_project(&ts, project_id).await;
        let mut activity = create_test_activity(
            project_id,
            "qwen3-vl:8b",
            vec![Uuid::new_v4(), Uuid::new_v4()],
        );

        ts.store.create(activity.clone()).await.unwrap();
        let retrieved = ts.store.get(activity.activity_id).await.unwrap().unwrap();
        assert_eq!(retrieved.status, ActivityStatus::Queued);

        activity.start();
        ts.store.update(activity.clone()).await.unwrap();
        let retrieved = ts.store.get(activity.activity_id).await.unwrap().unwrap();
        assert_eq!(retrieved.status, ActivityStatus::Processing);
        assert!(retrieved.started_at.is_some());

        activity.complete();
        ts.store.update(activity.clone()).await.unwrap();
        let retrieved = ts.store.get(activity.activity_id).await.unwrap().unwrap();
        assert_eq!(retrieved.status, ActivityStatus::Completed);
        assert!(retrieved.completed_at.is_some());
        assert!(retrieved.is_finished());
    }

    #[tokio::test]
    async fn test_persistence_survives_reload() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let storage_path = temp_dir.path().to_path_buf();

        let project_id = Uuid::new_v4();
        let catalog_id = Uuid::new_v4();
        let activity1 = create_test_activity(project_id, "qwen3-vl:8b", vec![Uuid::new_v4()]);
        let activity2 = create_test_activity(project_id, "llava", vec![Uuid::new_v4()]);
        let activity1_id = activity1.activity_id;
        let activity2_id = activity2.activity_id;

        {
            let project_store: Arc<dyn ProjectStore> =
                Arc::new(FileSystemProjectStore::new(storage_path.clone()).await);
            let project = Project {
                project_id,
                catalog_id,
                name: "test".to_string(),
                context: "test".to_string(),
                created_at: Utc::now(),
            };
            project_store.create(project).await.unwrap();
            let store = FileSystemActivityStore::new(storage_path.clone(), project_store).await;
            store.create(activity1).await.unwrap();
            store.create(activity2).await.unwrap();
            assert_eq!(store.count_by_task(project_id).await.unwrap(), 2);
        }

        let project_store: Arc<dyn ProjectStore> =
            Arc::new(FileSystemProjectStore::new(storage_path.clone()).await);
        let store = FileSystemActivityStore::new(storage_path, project_store).await;

        assert_eq!(store.count_by_task(project_id).await.unwrap(), 2);
        assert!(store.exists(activity1_id).await.unwrap());
        assert!(store.exists(activity2_id).await.unwrap());

        let loaded = store.get(activity1_id).await.unwrap().unwrap();
        assert_eq!(loaded.model, "qwen3-vl:8b");
    }

    #[tokio::test]
    async fn test_update_persists() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let storage_path = temp_dir.path().to_path_buf();

        let project_id = Uuid::new_v4();
        let catalog_id = Uuid::new_v4();
        let mut activity = create_test_activity(project_id, "qwen3-vl:8b", vec![Uuid::new_v4()]);
        let activity_id = activity.activity_id;

        {
            let project_store: Arc<dyn ProjectStore> =
                Arc::new(FileSystemProjectStore::new(storage_path.clone()).await);
            let project = Project {
                project_id,
                catalog_id,
                name: "test".to_string(),
                context: "test".to_string(),
                created_at: Utc::now(),
            };
            project_store.create(project).await.unwrap();
            let store = FileSystemActivityStore::new(storage_path.clone(), project_store).await;
            store.create(activity.clone()).await.unwrap();

            activity.start();
            store.update(activity.clone()).await.unwrap();
        }

        let project_store: Arc<dyn ProjectStore> =
            Arc::new(FileSystemProjectStore::new(storage_path.clone()).await);
        let store = FileSystemActivityStore::new(storage_path, project_store).await;
        let loaded = store.get(activity_id).await.unwrap().unwrap();
        assert_eq!(loaded.status, ActivityStatus::Processing);
        assert!(loaded.started_at.is_some());
    }

    #[tokio::test]
    async fn test_delete_removes_from_filesystem() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let storage_path = temp_dir.path().to_path_buf();

        let project_id = Uuid::new_v4();
        let catalog_id = Uuid::new_v4();
        let activity = create_test_activity(project_id, "qwen3-vl:8b", vec![Uuid::new_v4()]);
        let activity_id = activity.activity_id;

        {
            let project_store: Arc<dyn ProjectStore> =
                Arc::new(FileSystemProjectStore::new(storage_path.clone()).await);
            let project = Project {
                project_id,
                catalog_id,
                name: "test".to_string(),
                context: "test".to_string(),
                created_at: Utc::now(),
            };
            project_store.create(project).await.unwrap();
            let store = FileSystemActivityStore::new(storage_path.clone(), project_store).await;
            store.create(activity).await.unwrap();
            store.delete(activity_id).await.unwrap();
        }

        let project_store: Arc<dyn ProjectStore> =
            Arc::new(FileSystemProjectStore::new(storage_path.clone()).await);
        let store = FileSystemActivityStore::new(storage_path, project_store).await;
        assert_eq!(store.count_by_task(project_id).await.unwrap(), 0);
        assert!(!store.exists(activity_id).await.unwrap());
    }

    /// Simulates a failure writing the `.tmp` file by making the jobs directory
    /// non-writable, then verifies the in-memory activity is unchanged.
    #[cfg(unix)]
    #[tokio::test]
    async fn test_update_memory_unchanged_when_tmp_write_fails() {
        use std::os::unix::fs::PermissionsExt;

        let ts = create_store().await;
        let project_id = Uuid::new_v4();
        let catalog_id = setup_project(&ts, project_id).await;
        let activity = create_test_activity(project_id, "qwen3-vl:8b", vec![Uuid::new_v4()]);
        ts.store.create(activity.clone()).await.unwrap();

        let jobs_dir = ts.store.layout.jobs_dir(catalog_id, project_id);
        std::fs::set_permissions(&jobs_dir, std::fs::Permissions::from_mode(0o555)).unwrap();

        let mut updated = activity.clone();
        updated.start();
        let result = ts.store.update(updated).await;

        std::fs::set_permissions(&jobs_dir, std::fs::Permissions::from_mode(0o755)).unwrap();

        assert!(result.is_err());
        let in_memory = ts.store.get(activity.activity_id).await.unwrap().unwrap();
        assert_eq!(in_memory.status, activity.status);
    }

    /// Simulates a rename failure by replacing the activity JSON file with a directory,
    /// then verifies the in-memory activity is unchanged.
    #[cfg(unix)]
    #[tokio::test]
    async fn test_update_memory_unchanged_when_rename_fails() {
        let ts = create_store().await;
        let project_id = Uuid::new_v4();
        let catalog_id = setup_project(&ts, project_id).await;
        let activity = create_test_activity(project_id, "qwen3-vl:8b", vec![Uuid::new_v4()]);
        ts.store.create(activity.clone()).await.unwrap();

        let activity_path =
            ts.store
                .layout
                .job_file_path(catalog_id, project_id, activity.activity_id);
        tokio::fs::remove_file(&activity_path).await.unwrap();
        tokio::fs::create_dir(&activity_path).await.unwrap();

        let mut updated = activity.clone();
        updated.start();
        let result = ts.store.update(updated).await;

        tokio::fs::remove_dir(&activity_path).await.unwrap();

        assert!(result.is_err());
        let in_memory = ts.store.get(activity.activity_id).await.unwrap().unwrap();
        assert_eq!(in_memory.status, activity.status);
    }

    /// Simulates a filesystem failure during delete by making the jobs directory
    /// non-writable, then verifies the activity is rolled back into memory.
    #[cfg(unix)]
    #[tokio::test]
    async fn test_delete_memory_rolled_back_when_filesystem_fails() {
        use std::os::unix::fs::PermissionsExt;

        let ts = create_store().await;
        let project_id = Uuid::new_v4();
        let catalog_id = setup_project(&ts, project_id).await;
        let activity = create_test_activity(project_id, "qwen3-vl:8b", vec![Uuid::new_v4()]);
        ts.store.create(activity.clone()).await.unwrap();

        let jobs_dir = ts.store.layout.jobs_dir(catalog_id, project_id);
        std::fs::set_permissions(&jobs_dir, std::fs::Permissions::from_mode(0o555)).unwrap();

        let result = ts.store.delete(activity.activity_id).await;

        std::fs::set_permissions(&jobs_dir, std::fs::Permissions::from_mode(0o755)).unwrap();

        assert!(result.is_err());
        let in_memory = ts.store.get(activity.activity_id).await.unwrap();
        assert!(in_memory.is_some());
    }

    #[tokio::test]
    async fn test_multiple_projects_with_activities_load_correctly() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let storage_path = temp_dir.path().to_path_buf();

        let project_a = Uuid::new_v4();
        let project_b = Uuid::new_v4();
        let catalog_id = Uuid::new_v4();
        let activity1 = create_test_activity(project_a, "qwen3-vl:8b", vec![Uuid::new_v4()]);
        let activity2 = create_test_activity(project_a, "llava", vec![Uuid::new_v4()]);
        let activity3 = create_test_activity(project_b, "qwen3-vl:8b", vec![Uuid::new_v4()]);
        let activity4 = create_test_activity(project_b, "llava", vec![Uuid::new_v4()]);

        {
            let project_store: Arc<dyn ProjectStore> =
                Arc::new(FileSystemProjectStore::new(storage_path.clone()).await);
            for &pid in &[project_a, project_b] {
                let project = Project {
                    project_id: pid,
                    catalog_id,
                    name: "test".to_string(),
                    context: "test".to_string(),
                    created_at: Utc::now(),
                };
                project_store.create(project).await.unwrap();
            }
            let store = FileSystemActivityStore::new(storage_path.clone(), project_store).await;
            store.create(activity1).await.unwrap();
            store.create(activity2).await.unwrap();
            store.create(activity3).await.unwrap();
            store.create(activity4).await.unwrap();
        }

        let project_store: Arc<dyn ProjectStore> =
            Arc::new(FileSystemProjectStore::new(storage_path.clone()).await);
        let store = FileSystemActivityStore::new(storage_path, project_store).await;
        assert_eq!(store.list().await.unwrap().len(), 4);
        assert_eq!(store.count_by_task(project_a).await.unwrap(), 2);
        assert_eq!(store.count_by_task(project_b).await.unwrap(), 2);
    }

    /// Sets up a project on disk with an activity and returns the activity file path.
    async fn setup_project_and_activity(
        storage_path: &PathBuf,
        project_id: Uuid,
        catalog_id: Uuid,
        activity: &Activity,
    ) -> PathBuf {
        let project_store: Arc<dyn ProjectStore> =
            Arc::new(FileSystemProjectStore::new(storage_path.clone()).await);
        let project = Project {
            project_id,
            catalog_id,
            name: "test".to_string(),
            context: "ctx".to_string(),
            created_at: Utc::now(),
        };
        project_store.create(project).await.unwrap();
        let store = FileSystemActivityStore::new(storage_path.clone(), project_store).await;
        store.create(activity.clone()).await.unwrap();
        store
            .layout
            .job_file_path(catalog_id, project_id, activity.activity_id)
    }

    #[tokio::test]
    async fn test_load_quarantines_corrupt_activity_file() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let storage_path = temp_dir.path().to_path_buf();
        let project_id = Uuid::new_v4();
        let catalog_id = Uuid::new_v4();
        let activity = create_test_activity(project_id, "llava", vec![Uuid::new_v4()]);
        let activity_id = activity.activity_id;

        let activity_file =
            setup_project_and_activity(&storage_path, project_id, catalog_id, &activity).await;
        tokio::fs::write(&activity_file, b"not valid json")
            .await
            .unwrap();

        let project_store: Arc<dyn ProjectStore> =
            Arc::new(FileSystemProjectStore::new(storage_path.clone()).await);
        let store = FileSystemActivityStore::new(storage_path, project_store).await;

        assert!(!store.exists(activity_id).await.unwrap());
        assert!(!activity_file.exists());
        assert!(store.layout.quarantine_path_for(&activity_file).exists());
    }

    #[tokio::test]
    async fn test_load_quarantines_inconsistent_activity_id() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let storage_path = temp_dir.path().to_path_buf();
        let project_id = Uuid::new_v4();
        let catalog_id = Uuid::new_v4();
        let activity = create_test_activity(project_id, "llava", vec![Uuid::new_v4()]);

        let activity_file =
            setup_project_and_activity(&storage_path, project_id, catalog_id, &activity).await;

        let mut tampered = activity.clone();
        tampered.activity_id = Uuid::new_v4();
        tokio::fs::write(&activity_file, serde_json::to_string(&tampered).unwrap())
            .await
            .unwrap();

        let project_store: Arc<dyn ProjectStore> =
            Arc::new(FileSystemProjectStore::new(storage_path.clone()).await);
        let store = FileSystemActivityStore::new(storage_path, project_store).await;

        assert_eq!(store.count_by_task(project_id).await.unwrap(), 0);
        assert!(!activity_file.exists());
        assert!(store.layout.quarantine_path_for(&activity_file).exists());
    }

    #[tokio::test]
    async fn test_load_quarantines_inconsistent_project_id() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let storage_path = temp_dir.path().to_path_buf();
        let project_id = Uuid::new_v4();
        let catalog_id = Uuid::new_v4();
        let activity = create_test_activity(project_id, "llava", vec![Uuid::new_v4()]);

        let activity_file =
            setup_project_and_activity(&storage_path, project_id, catalog_id, &activity).await;

        let mut tampered = activity.clone();
        tampered.project_id = Uuid::new_v4();
        tokio::fs::write(&activity_file, serde_json::to_string(&tampered).unwrap())
            .await
            .unwrap();

        let project_store: Arc<dyn ProjectStore> =
            Arc::new(FileSystemProjectStore::new(storage_path.clone()).await);
        let store = FileSystemActivityStore::new(storage_path, project_store).await;

        assert_eq!(store.count_by_task(project_id).await.unwrap(), 0);
        assert!(!activity_file.exists());
        assert!(store.layout.quarantine_path_for(&activity_file).exists());
    }

    #[tokio::test]
    async fn test_load_valid_activity_not_quarantined() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let storage_path = temp_dir.path().to_path_buf();
        let project_id = Uuid::new_v4();
        let catalog_id = Uuid::new_v4();
        let activity = create_test_activity(project_id, "llava", vec![Uuid::new_v4()]);
        let activity_id = activity.activity_id;

        setup_project_and_activity(&storage_path, project_id, catalog_id, &activity).await;

        let project_store: Arc<dyn ProjectStore> =
            Arc::new(FileSystemProjectStore::new(storage_path.clone()).await);
        let store = FileSystemActivityStore::new(storage_path, project_store).await;

        assert!(store.exists(activity_id).await.unwrap());
        assert!(!store.layout.quarantine_dir().exists());
    }
}
