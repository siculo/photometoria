// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 The Photometoria contributors

//! Filesystem-backed implementation of TaskStore
//!
//! This module provides a thread-safe implementation of the TaskStore trait
//! with full persistence to the filesystem. Task metadata is stored as JSON files.

use async_trait::async_trait;
use dashmap::DashMap;
use dashmap::mapref::entry::Entry;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::models::Task;

use super::utils::parse_uuid_from_dir;
use super::{FileSystemLayout, TaskStore, TaskStoreError, TaskStoreResult};

fn sorted_tasks(iter: impl Iterator<Item = Task>) -> Vec<Task> {
    let mut tasks: Vec<Task> = iter.collect();
    tasks.sort_by_key(|t| t.created_at);
    tasks
}

/// Filesystem-backed implementation of TaskStore with full persistence.
///
/// This implementation stores task metadata both in memory (using DashMap for
/// fast concurrent access) and on the filesystem (as JSON files for persistence).
///
/// ## Characteristics
///
/// - **Thread-safe**: Supports concurrent access from multiple Tokio tasks
/// - **Lock-free reads**: Get operations don't acquire locks
/// - **Fine-grained locking**: Writes lock only the specific shard
/// - **Full persistence**: Task metadata survives server restarts
/// - **Filesystem integration**: Creates/removes task directories and metadata files
///
/// ## Persistence Strategy
///
/// - Metadata is written to disk after each create/update operation
/// - On startup, all existing task.json files are loaded into memory
/// - Deleting a task removes both the in-memory entry and the entire directory
///
/// For details on the filesystem layout, see [`FileSystemLayout`]
pub struct FileSystemTaskStore {
    tasks: Arc<DashMap<Uuid, Task>>,
    layout: FileSystemLayout,
}

impl FileSystemTaskStore {
    /// Creates a new filesystem-backed task store.
    ///
    /// This constructor loads all existing tasks from the filesystem.
    /// Any errors during loading are logged but don't prevent startup.
    ///
    /// # Arguments
    /// * `storage_path` - Base path for storing task directories
    pub async fn new(storage_path: PathBuf) -> Self {
        let store = Self {
            tasks: Arc::new(DashMap::new()),
            layout: FileSystemLayout::new(storage_path),
        };
        store.load_all().await;
        store
    }

    /// Loads all tasks from the filesystem into memory.
    ///
    /// Scans the catalog hierarchy: `catalogs/{catalog_id}/tasks/{task_id}/task.json`
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
            let (n_loaded, n_errors) = self.load_catalog_tasks(catalog_id).await;
            loaded += n_loaded;
            errors += n_errors;
        }

        info!("Loaded {} tasks from filesystem ({} errors)", loaded, errors);
    }

    /// Loads all tasks for a single catalog from the filesystem.
    ///
    /// Returns the count of successfully loaded tasks and errors encountered.
    async fn load_catalog_tasks(&self, catalog_id: Uuid) -> (usize, usize) {
        let task_files = match self.layout.scan_task_json_files(catalog_id).await {
            Ok(files) => files,
            Err(e) => {
                warn!(
                    "Failed to scan task files for catalog {}: {}",
                    catalog_id, e
                );
                return (0, 1);
            }
        };

        let mut loaded = 0;
        let mut errors = 0;

        for task_json in task_files {
            match self.load_task_from_file(&task_json).await {
                Ok(task) => {
                    self.tasks.insert(task.task_id, task);
                    loaded += 1;
                }
                Err(e) => {
                    warn!("Failed to load task from {:?}: {}", task_json, e);
                    errors += 1;
                }
            }
        }

        (loaded, errors)
    }

    /// Loads a single task from a JSON file.
    async fn load_task_from_file(&self, path: &Path) -> TaskStoreResult<Task> {
        let content = tokio::fs::read_to_string(path).await.map_err(|e| {
            TaskStoreError::StorageError(format!("Failed to read task file: {}", e))
        })?;

        serde_json::from_str(&content)
            .map_err(|e| TaskStoreError::StorageError(format!("Failed to parse task JSON: {}", e)))
    }

    /// Saves a task's metadata to the filesystem atomically.
    ///
    /// Writes to a temporary file first, then renames to the final path.
    /// This ensures the target file is never left in a partially-written state.
    async fn save_task_to_file(&self, task: &Task) -> TaskStoreResult<()> {
        let path = self.layout.task_json_path(task.catalog_id, task.task_id);
        let tmp_path = path.with_extension("tmp");
        let content = serde_json::to_string_pretty(task).map_err(|e| {
            TaskStoreError::StorageError(format!("Failed to serialize task: {}", e))
        })?;

        tokio::fs::write(&tmp_path, &content).await.map_err(|e| {
            error!("Failed to write temporary task file {:?}: {}", tmp_path, e);
            TaskStoreError::StorageError(format!("Failed to write task file: {}", e))
        })?;

        tokio::fs::rename(&tmp_path, &path).await.map_err(|e| {
            error!("Failed to rename {:?} -> {:?}: {}", tmp_path, path, e);
            TaskStoreError::StorageError(format!("Failed to atomically save task file: {}", e))
        })?;

        debug!("Saved task metadata to {:?}", path);
        Ok(())
    }
}

#[async_trait]
impl TaskStore for FileSystemTaskStore {
    async fn create(&self, task: Task) -> TaskStoreResult<Task> {
        let task_id = task.task_id;

        debug!("Attempting to create task: {}", task_id);

        match self.tasks.entry(task_id) {
            Entry::Occupied(_) => Err(TaskStoreError::AlreadyExists(task_id)),
            Entry::Vacant(entry) => {
                let task_dir = self
                    .layout
                    .ensure_task_dir(task.catalog_id, task.task_id)
                    .await
                    .map_err(|e| {
                        error!("Failed to create task directory: {}", e);
                        TaskStoreError::StorageError(format!(
                            "Failed to create task directory: {}",
                            e
                        ))
                    })?;
                debug!("Created task directory: {:?}", task_dir);

                self.save_task_to_file(&task).await?;

                entry.insert(task.clone());
                info!(
                    "Task created successfully: {} (context: '{}')",
                    task_id, task.context
                );
                Ok(task)
            }
        }
    }

    async fn get(&self, task_id: Uuid) -> TaskStoreResult<Option<Task>> {
        debug!("Retrieving task: {}", task_id);

        Ok(self.tasks.get(&task_id).map(|entry| entry.value().clone()))
    }

    async fn list(&self) -> TaskStoreResult<Vec<Task>> {
        debug!("Listing all tasks");

        let tasks = sorted_tasks(self.tasks.iter().map(|e| e.value().clone()));

        info!("Listed {} tasks", tasks.len());
        Ok(tasks)
    }

    async fn update(&self, task: Task) -> TaskStoreResult<Task> {
        let task_id = task.task_id;

        debug!("Updating task: {}", task_id);

        match self.tasks.get_mut(&task_id) {
            Some(mut entry) => {
                self.save_task_to_file(&task).await?;

                let old_context = entry.context.clone();
                *entry = task.clone();
                info!(
                    "Task updated successfully: {} (context: '{}' -> '{}')",
                    task_id, old_context, task.context
                );
                Ok(task)
            }
            None => Err(TaskStoreError::NotFound(task_id)),
        }
    }

    async fn delete(&self, task_id: Uuid) -> TaskStoreResult<()> {
        debug!("Deleting task: {}", task_id);

        let (_, task) = match self.tasks.remove(&task_id) {
            Some(entry) => entry,
            None => return Err(TaskStoreError::NotFound(task_id)),
        };

        let task_dir = self.layout.task_dir(task.catalog_id, task.task_id);
        match tokio::fs::remove_dir_all(&task_dir).await {
            Ok(()) => debug!("Removed task directory: {:?}", task_dir),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                error!("Failed to remove task directory {:?}: {}", task_dir, e);
                self.tasks.insert(task_id, task);
                return Err(TaskStoreError::StorageError(format!(
                    "Failed to remove task directory: {}",
                    e
                )));
            }
        }

        info!(
            "Task deleted successfully: {} (context: '{}')",
            task_id, task.context
        );
        Ok(())
    }

    async fn exists(&self, task_id: Uuid) -> TaskStoreResult<bool> {
        debug!("Checking if task exists: {}", task_id);

        Ok(self.tasks.contains_key(&task_id))
    }

    async fn count(&self) -> TaskStoreResult<usize> {
        debug!("Counting tasks");

        Ok(self.tasks.len())
    }

    async fn list_by_catalog(&self, catalog_id: Uuid) -> TaskStoreResult<Vec<Task>> {
        debug!("Listing tasks for catalog: {}", catalog_id);

        let tasks = sorted_tasks(
            self.tasks
                .iter()
                .filter(|e| e.value().catalog_id == catalog_id)
                .map(|e| e.value().clone()),
        );

        info!("Listed {} tasks for catalog {}", tasks.len(), catalog_id);
        Ok(tasks)
    }

    async fn find_by_name(&self, catalog_id: Uuid, name: &str) -> TaskStoreResult<Option<Task>> {
        debug!("Finding task by name '{}' in catalog {}", name, catalog_id);

        Ok(self
            .tasks
            .iter()
            .find(|entry| entry.value().catalog_id == catalog_id && entry.value().name == name)
            .map(|entry| entry.value().clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Duration, Utc};
    use tempfile::TempDir;
    use uuid::Uuid;

    struct TestStore {
        store: FileSystemTaskStore,
        _temp_dir: TempDir,
    }

    async fn create_store() -> TestStore {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let store = FileSystemTaskStore::new(temp_dir.path().to_path_buf()).await;
        TestStore {
            store,
            _temp_dir: temp_dir,
        }
    }

    fn create_test_task_with_timestamp(name: &str, timestamp: DateTime<Utc>) -> Task {
        Task {
            task_id: Uuid::new_v4(),
            catalog_id: Uuid::new_v4(),
            name: name.to_string(),
            context: format!("context for {}", name),
            created_at: timestamp,
        }
    }

    fn create_test_task(name: &str) -> Task {
        Task::new(
            Uuid::new_v4(),
            name.to_string(),
            format!("context for {}", name),
        )
    }

    #[tokio::test]
    async fn test_create_task() {
        let ts = create_store().await;
        let task = create_test_task("vacation in SF");

        let result = ts.store.create(task.clone()).await;

        assert!(result.is_ok());
        let created = result.unwrap();
        assert_eq!(created.task_id, task.task_id);
        assert_eq!(created.context, task.context);

        let exists = ts.store.exists(task.task_id).await.unwrap();
        assert!(exists);

        assert!(
            ts.store
                .layout
                .task_dir(task.catalog_id, task.task_id)
                .exists()
        );

        assert!(
            ts.store
                .layout
                .task_json_path(task.catalog_id, task.task_id)
                .exists()
        );
    }

    #[tokio::test]
    async fn test_create_duplicate_fails() {
        let ts = create_store().await;
        let task = create_test_task("vacation in SF");

        ts.store.create(task.clone()).await.unwrap();

        let result = ts.store.create(task.clone()).await;

        assert!(result.is_err());
        match result {
            Err(TaskStoreError::AlreadyExists(id)) => {
                assert_eq!(id, task.task_id);
            }
            _ => panic!("Expected AlreadyExists error"),
        }
    }

    #[tokio::test]
    async fn test_get_existing_task() {
        let ts = create_store().await;
        let task = create_test_task("vacation in SF");

        ts.store.create(task.clone()).await.unwrap();

        let result = ts.store.get(task.task_id).await.unwrap();

        assert!(result.is_some());
        let retrieved = result.unwrap();
        assert_eq!(retrieved.task_id, task.task_id);
        assert_eq!(retrieved.context, task.context);
        assert_eq!(retrieved.created_at, task.created_at);
    }

    #[tokio::test]
    async fn test_get_nonexistent_task() {
        let ts = create_store().await;

        let result = ts.store.get(Uuid::new_v4()).await.unwrap();

        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_list_tasks_ordered_by_created_at() {
        let ts = create_store().await;

        let now = Utc::now();
        let task1 = create_test_task_with_timestamp("First", now);
        let task2 = create_test_task_with_timestamp("Second", now + Duration::seconds(1));
        let task3 = create_test_task_with_timestamp("Third", now + Duration::seconds(2));

        ts.store.create(task2.clone()).await.unwrap();
        ts.store.create(task1.clone()).await.unwrap();
        ts.store.create(task3.clone()).await.unwrap();

        let tasks = ts.store.list().await.unwrap();

        assert_eq!(tasks.len(), 3);
        assert_eq!(tasks[0].name, "First");
        assert_eq!(tasks[1].name, "Second");
        assert_eq!(tasks[2].name, "Third");
        assert_eq!(tasks[0].task_id, task1.task_id);
        assert_eq!(tasks[1].task_id, task2.task_id);
        assert_eq!(tasks[2].task_id, task3.task_id);
    }

    #[tokio::test]
    async fn test_update_task() {
        let ts = create_store().await;
        let task = create_test_task("vacation in SF");

        ts.store.create(task.clone()).await.unwrap();

        let mut updated_task = task.clone();
        updated_task.context = "Updated context".to_string();

        let result = ts.store.update(updated_task.clone()).await;

        assert!(result.is_ok());
        let updated = result.unwrap();
        assert_eq!(updated.context, "Updated context");

        let retrieved = ts.store.get(task.task_id).await.unwrap().unwrap();
        assert_eq!(retrieved.context, "Updated context");
    }

    #[tokio::test]
    async fn test_update_nonexistent_fails() {
        let ts = create_store().await;
        let task = create_test_task("vacation in SF");

        let result = ts.store.update(task.clone()).await;

        assert!(result.is_err());
        match result {
            Err(TaskStoreError::NotFound(id)) => {
                assert_eq!(id, task.task_id);
            }
            _ => panic!("Expected NotFound error"),
        }
    }

    #[tokio::test]
    async fn test_delete_task() {
        let ts = create_store().await;
        let task = create_test_task("vacation in SF");

        ts.store.create(task.clone()).await.unwrap();
        let task_dir = ts.store.layout.task_dir(task.catalog_id, task.task_id);
        assert!(task_dir.exists());

        let result = ts.store.delete(task.task_id).await;

        assert!(result.is_ok());

        let retrieved = ts.store.get(task.task_id).await.unwrap();
        assert!(retrieved.is_none());

        assert!(!task_dir.exists());
    }

    #[tokio::test]
    async fn test_delete_nonexistent_fails() {
        let ts = create_store().await;

        let nonexistent_id = Uuid::new_v4();
        let result = ts.store.delete(nonexistent_id).await;

        assert!(result.is_err());
        match result {
            Err(TaskStoreError::NotFound(id)) => {
                assert_eq!(id, nonexistent_id);
            }
            _ => panic!("Expected NotFound error"),
        }
    }

    #[tokio::test]
    async fn test_exists() {
        let ts = create_store().await;
        let task = create_test_task("vacation in SF");

        let exists = ts.store.exists(task.task_id).await.unwrap();
        assert!(!exists);

        ts.store.create(task.clone()).await.unwrap();

        let exists = ts.store.exists(task.task_id).await.unwrap();
        assert!(exists);

        let exists = ts.store.exists(Uuid::new_v4()).await.unwrap();
        assert!(!exists);
    }

    #[tokio::test]
    async fn test_count() {
        let ts = create_store().await;

        let count = ts.store.count().await.unwrap();
        assert_eq!(count, 0);

        let task1 = create_test_task("First");
        let task2 = create_test_task("Second");
        let task3 = create_test_task("Third");

        ts.store.create(task1.clone()).await.unwrap();
        ts.store.create(task2.clone()).await.unwrap();
        ts.store.create(task3.clone()).await.unwrap();

        let count = ts.store.count().await.unwrap();
        assert_eq!(count, 3);

        ts.store.delete(task1.task_id).await.unwrap();

        let count = ts.store.count().await.unwrap();
        assert_eq!(count, 2);
    }

    #[tokio::test]
    async fn test_persistence_survives_reload() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let storage_path = temp_dir.path().to_path_buf();

        let task1 = create_test_task("First task");
        let task2 = create_test_task("Second task");
        let task1_id = task1.task_id;
        let task2_id = task2.task_id;

        {
            let store = FileSystemTaskStore::new(storage_path.clone()).await;
            store.create(task1).await.unwrap();
            store.create(task2).await.unwrap();
            assert_eq!(store.count().await.unwrap(), 2);
        }

        let store = FileSystemTaskStore::new(storage_path).await;

        assert_eq!(store.count().await.unwrap(), 2);
        assert!(store.exists(task1_id).await.unwrap());
        assert!(store.exists(task2_id).await.unwrap());

        let loaded_task1 = store.get(task1_id).await.unwrap().unwrap();
        assert_eq!(loaded_task1.name, "First task");
    }

    #[tokio::test]
    async fn test_update_persists() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let storage_path = temp_dir.path().to_path_buf();

        let task = create_test_task("Original context");
        let task_id = task.task_id;

        {
            let store = FileSystemTaskStore::new(storage_path.clone()).await;
            store.create(task.clone()).await.unwrap();

            let mut updated = task.clone();
            updated.context = "Updated context".to_string();
            store.update(updated).await.unwrap();
        }

        let store = FileSystemTaskStore::new(storage_path).await;
        let loaded = store.get(task_id).await.unwrap().unwrap();
        assert_eq!(loaded.context, "Updated context");
    }

    /// Simulates a filesystem failure during delete by making the task directory
    /// non-removable, then verifies the task is rolled back into memory.
    #[cfg(unix)]
    #[tokio::test]
    async fn test_delete_memory_rolled_back_when_filesystem_fails() {
        use std::os::unix::fs::PermissionsExt;

        let ts = create_store().await;
        let task = create_test_task("to delete");
        ts.store.create(task.clone()).await.unwrap();

        let task_dir = ts.store.layout.task_dir(task.catalog_id, task.task_id);
        let parent_dir = task_dir.parent().unwrap();
        std::fs::set_permissions(parent_dir, std::fs::Permissions::from_mode(0o555)).unwrap();

        let result = ts.store.delete(task.task_id).await;

        std::fs::set_permissions(parent_dir, std::fs::Permissions::from_mode(0o755)).unwrap();

        assert!(result.is_err());
        let in_memory = ts.store.get(task.task_id).await.unwrap();
        assert!(in_memory.is_some());
    }

    /// Simulates a failure writing the `.tmp` file by making the task directory
    /// non-writable, then verifies the in-memory task is unchanged.
    #[cfg(unix)]
    #[tokio::test]
    async fn test_update_memory_unchanged_when_tmp_write_fails() {
        use std::os::unix::fs::PermissionsExt;

        let ts = create_store().await;
        let task = create_test_task("original");
        ts.store.create(task.clone()).await.unwrap();

        let task_dir = ts.store.layout.task_dir(task.catalog_id, task.task_id);
        std::fs::set_permissions(&task_dir, std::fs::Permissions::from_mode(0o555)).unwrap();

        let mut updated = task.clone();
        updated.context = "updated context".to_string();
        let result = ts.store.update(updated).await;

        std::fs::set_permissions(&task_dir, std::fs::Permissions::from_mode(0o755)).unwrap();

        assert!(result.is_err());
        let in_memory = ts.store.get(task.task_id).await.unwrap().unwrap();
        assert_eq!(in_memory.context, task.context);
    }

    /// Simulates a rename failure by replacing `task.json` with a directory,
    /// then verifies the in-memory task is unchanged.
    #[cfg(unix)]
    #[tokio::test]
    async fn test_update_memory_unchanged_when_rename_fails() {
        let ts = create_store().await;
        let task = create_test_task("original");
        ts.store.create(task.clone()).await.unwrap();

        let task_json = ts.store.layout.task_json_path(task.catalog_id, task.task_id);
        tokio::fs::remove_file(&task_json).await.unwrap();
        tokio::fs::create_dir(&task_json).await.unwrap();

        let mut updated = task.clone();
        updated.context = "updated context".to_string();
        let result = ts.store.update(updated).await;

        tokio::fs::remove_dir(&task_json).await.unwrap();

        assert!(result.is_err());
        let in_memory = ts.store.get(task.task_id).await.unwrap().unwrap();
        assert_eq!(in_memory.context, task.context);
    }

    #[tokio::test]
    async fn test_list_by_catalog_returns_only_matching_tasks() {
        let ts = create_store().await;
        let catalog_a = Uuid::new_v4();
        let catalog_b = Uuid::new_v4();

        let task1 = Task::new(catalog_a, "Task A1".to_string(), "ctx".to_string());
        let task2 = Task::new(catalog_a, "Task A2".to_string(), "ctx".to_string());
        let task3 = Task::new(catalog_b, "Task B1".to_string(), "ctx".to_string());

        ts.store.create(task1).await.unwrap();
        ts.store.create(task2).await.unwrap();
        ts.store.create(task3).await.unwrap();

        let catalog_a_tasks = ts.store.list_by_catalog(catalog_a).await.unwrap();
        assert_eq!(catalog_a_tasks.len(), 2);
        assert!(catalog_a_tasks.iter().all(|t| t.catalog_id == catalog_a));

        let catalog_b_tasks = ts.store.list_by_catalog(catalog_b).await.unwrap();
        assert_eq!(catalog_b_tasks.len(), 1);
        assert_eq!(catalog_b_tasks[0].name, "Task B1");
    }

    #[tokio::test]
    async fn test_list_by_catalog_empty_for_unknown_catalog() {
        let ts = create_store().await;

        let tasks = ts.store.list_by_catalog(Uuid::new_v4()).await.unwrap();
        assert!(tasks.is_empty());
    }

    #[tokio::test]
    async fn test_find_by_name_scoped_to_catalog() {
        let ts = create_store().await;
        let catalog_a = Uuid::new_v4();
        let catalog_b = Uuid::new_v4();

        let task_a = Task::new(catalog_a, "Shared Name".to_string(), "ctx a".to_string());
        let task_b = Task::new(catalog_b, "Shared Name".to_string(), "ctx b".to_string());

        ts.store.create(task_a.clone()).await.unwrap();
        ts.store.create(task_b.clone()).await.unwrap();

        let found_a = ts
            .store
            .find_by_name(catalog_a, "Shared Name")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(found_a.catalog_id, catalog_a);
        assert_eq!(found_a.context, "ctx a");

        let found_b = ts
            .store
            .find_by_name(catalog_b, "Shared Name")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(found_b.catalog_id, catalog_b);
        assert_eq!(found_b.context, "ctx b");
    }

    #[tokio::test]
    async fn test_find_by_name_not_found_in_different_catalog() {
        let ts = create_store().await;
        let catalog_a = Uuid::new_v4();
        let catalog_b = Uuid::new_v4();

        let task = Task::new(catalog_a, "Only in A".to_string(), "ctx".to_string());
        ts.store.create(task).await.unwrap();

        let result = ts.store.find_by_name(catalog_b, "Only in A").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_delete_removes_from_filesystem() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let storage_path = temp_dir.path().to_path_buf();

        let task = create_test_task("Task to delete");
        let task_id = task.task_id;

        {
            let store = FileSystemTaskStore::new(storage_path.clone()).await;
            store.create(task).await.unwrap();
            store.delete(task_id).await.unwrap();
        }

        let store = FileSystemTaskStore::new(storage_path).await;
        assert_eq!(store.count().await.unwrap(), 0);
        assert!(!store.exists(task_id).await.unwrap());
    }
}
