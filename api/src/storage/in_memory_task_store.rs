//! In-memory implementation of TaskStore using DashMap
//!
//! This module provides a thread-safe in-memory implementation of the TaskStore trait
//! using DashMap for concurrent access without global locks.

use async_trait::async_trait;
use dashmap::DashMap;
use dashmap::mapref::entry::Entry;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{debug, error, info};

use crate::models::Task;

use super::{TaskStore, TaskStoreError, TaskStoreResult};

// ============================================================================
// InMemoryTaskStore Implementation
// ============================================================================

/// In-memory implementation of TaskStore using DashMap for thread-safe concurrent access.
///
/// This implementation stores tasks in memory using DashMap, which provides excellent
/// concurrent read/write performance through internal sharding.
///
/// Task directories are created on the filesystem when tasks are created, and removed
/// when tasks are deleted. This provides storage for photos associated with each task.
///
/// ## Characteristics
///
/// - **Thread-safe**: Supports concurrent access from multiple Tokio tasks
/// - **Lock-free reads**: Get operations don't acquire locks
/// - **Fine-grained locking**: Writes lock only the specific shard
/// - **No persistence**: Task metadata is lost when the server restarts
/// - **Filesystem integration**: Creates/removes task directories
///
/// ## Directory Structure
///
/// ```text
/// {storage_path}/
/// └── tasks/
///     ├── {task_id_1}/
///     ├── {task_id_2}/
///     └── ...
/// ```
///
/// ## Use Cases
///
/// - Development and testing
/// - Single-user scenarios
/// - Prototyping before adding database persistence
pub struct InMemoryTaskStore {
    tasks: Arc<DashMap<String, Task>>,
    storage_path: PathBuf,
}

impl InMemoryTaskStore {
    /// Creates a new empty in-memory task store.
    ///
    /// # Arguments
    /// * `storage_path` - Base path for storing task directories
    pub fn new(storage_path: PathBuf) -> Self {
        Self {
            tasks: Arc::new(DashMap::new()),
            storage_path,
        }
    }

    /// Returns the directory path for a specific task.
    fn task_dir(&self, task_id: &str) -> PathBuf {
        self.storage_path.join("tasks").join(task_id)
    }
}

// ============================================================================
// TaskStore Trait Implementation
// ============================================================================

#[async_trait]
impl TaskStore for InMemoryTaskStore {
    async fn create(&self, task: Task) -> TaskStoreResult<Task> {
        let task_id = task.task_id.clone();

        debug!("Attempting to create task: {}", task_id);

        // Use entry API to atomically check and insert
        match self.tasks.entry(task_id.clone()) {
            Entry::Occupied(_) => Err(TaskStoreError::AlreadyExists(task_id)),
            Entry::Vacant(entry) => {
                // Create task directory on filesystem
                let task_dir = self.task_dir(&task_id);
                if let Err(e) = tokio::fs::create_dir_all(&task_dir).await {
                    error!("Failed to create task directory {:?}: {}", task_dir, e);
                    return Err(TaskStoreError::StorageError(format!(
                        "Failed to create task directory: {}",
                        e
                    )));
                }
                debug!("Created task directory: {:?}", task_dir);

                entry.insert(task.clone());
                info!(
                    "Task created successfully: {} (context: '{}')",
                    task_id, task.context
                );
                Ok(task)
            }
        }
    }

    async fn get(&self, task_id: &str) -> TaskStoreResult<Option<Task>> {
        debug!("Retrieving task: {}", task_id);

        // Lock-free read with DashMap
        Ok(self.tasks.get(task_id).map(|entry| entry.value().clone()))
    }

    async fn list(&self) -> TaskStoreResult<Vec<Task>> {
        debug!("Listing all tasks");

        // Collect all tasks into a vector
        let mut tasks: Vec<Task> = self
            .tasks
            .iter()
            .map(|entry| entry.value().clone())
            .collect();

        // Sort by created_at (oldest first) for deterministic output
        tasks.sort_by_key(|task| task.created_at);

        info!("Listed {} tasks", tasks.len());
        Ok(tasks)
    }

    async fn update(&self, task: Task) -> TaskStoreResult<Task> {
        let task_id = task.task_id.clone();

        debug!("Updating task: {}", task_id);

        // Use get_mut to modify in-place
        match self.tasks.get_mut(&task_id) {
            Some(mut entry) => {
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

    async fn delete(&self, task_id: &str) -> TaskStoreResult<()> {
        debug!("Deleting task: {}", task_id);

        // remove() returns Some((key, value)) if the entry existed
        match self.tasks.remove(task_id) {
            Some((_, task)) => {
                // Remove task directory and all its contents
                let task_dir = self.task_dir(task_id);
                if task_dir.exists() {
                    if let Err(e) = tokio::fs::remove_dir_all(&task_dir).await {
                        error!("Failed to remove task directory {:?}: {}", task_dir, e);
                        // Log but don't fail - the task metadata is already removed
                    } else {
                        debug!("Removed task directory: {:?}", task_dir);
                    }
                }

                info!(
                    "Task deleted successfully: {} (context: '{}')",
                    task_id, task.context
                );
                Ok(())
            }
            None => Err(TaskStoreError::NotFound(task_id.to_string())),
        }
    }

    async fn exists(&self, task_id: &str) -> TaskStoreResult<bool> {
        debug!("Checking if task exists: {}", task_id);

        // Lock-free existence check
        Ok(self.tasks.contains_key(task_id))
    }

    async fn count(&self) -> TaskStoreResult<usize> {
        debug!("Counting tasks");

        // O(1) operation using internal atomic counter
        Ok(self.tasks.len())
    }
}

// ============================================================================
// Unit Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Duration, Utc};
    use tempfile::TempDir;
    use uuid::Uuid;

    // Helper struct to keep temp dir alive during test
    struct TestStore {
        store: InMemoryTaskStore,
        _temp_dir: TempDir,
    }

    // Helper function to create a fresh store with temp directory
    fn create_store() -> TestStore {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let store = InMemoryTaskStore::new(temp_dir.path().to_path_buf());
        TestStore {
            store,
            _temp_dir: temp_dir,
        }
    }

    // Helper function to create a test task with specific timestamp
    fn create_test_task_with_timestamp(context: &str, timestamp: DateTime<Utc>) -> Task {
        Task {
            task_id: Uuid::new_v4().to_string(),
            context: context.to_string(),
            created_at: timestamp,
        }
    }

    // Helper function to create a test task with current timestamp
    fn create_test_task(context: &str) -> Task {
        Task::new(context.to_string())
    }

    #[tokio::test]
    async fn test_create_task() {
        let ts = create_store();
        let task = create_test_task("vacation in SF");

        let result = ts.store.create(task.clone()).await;

        assert!(result.is_ok());
        let created = result.unwrap();
        assert_eq!(created.task_id, task.task_id);
        assert_eq!(created.context, task.context);

        // Verify it exists in the store
        let exists = ts.store.exists(&task.task_id).await.unwrap();
        assert!(exists);

        // Verify directory was created
        assert!(ts.store.task_dir(&task.task_id).exists());
    }

    #[tokio::test]
    async fn test_create_duplicate_fails() {
        let ts = create_store();
        let task = create_test_task("vacation in SF");

        // First creation should succeed
        ts.store.create(task.clone()).await.unwrap();

        // Second creation with same ID should fail
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
        let ts = create_store();
        let task = create_test_task("vacation in SF");

        ts.store.create(task.clone()).await.unwrap();

        let result = ts.store.get(&task.task_id).await.unwrap();

        assert!(result.is_some());
        let retrieved = result.unwrap();
        assert_eq!(retrieved.task_id, task.task_id);
        assert_eq!(retrieved.context, task.context);
        assert_eq!(retrieved.created_at, task.created_at);
    }

    #[tokio::test]
    async fn test_get_nonexistent_task() {
        let ts = create_store();

        let result = ts.store.get("nonexistent_id").await.unwrap();

        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_list_tasks_ordered_by_created_at() {
        let ts = create_store();

        // Create tasks with explicit timestamps to control ordering
        let now = Utc::now();
        let task1 = create_test_task_with_timestamp("First", now);
        let task2 = create_test_task_with_timestamp("Second", now + Duration::seconds(1));
        let task3 = create_test_task_with_timestamp("Third", now + Duration::seconds(2));

        // Insert in random order
        ts.store.create(task2.clone()).await.unwrap();
        ts.store.create(task1.clone()).await.unwrap();
        ts.store.create(task3.clone()).await.unwrap();

        // List should return ordered by created_at (oldest first)
        let tasks = ts.store.list().await.unwrap();

        assert_eq!(tasks.len(), 3);
        assert_eq!(tasks[0].context, "First");
        assert_eq!(tasks[1].context, "Second");
        assert_eq!(tasks[2].context, "Third");
        assert_eq!(tasks[0].task_id, task1.task_id);
        assert_eq!(tasks[1].task_id, task2.task_id);
        assert_eq!(tasks[2].task_id, task3.task_id);
    }

    #[tokio::test]
    async fn test_update_task() {
        let ts = create_store();
        let task = create_test_task("vacation in SF");

        ts.store.create(task.clone()).await.unwrap();

        // Update the context
        let mut updated_task = task.clone();
        updated_task.context = "Updated context".to_string();

        let result = ts.store.update(updated_task.clone()).await;

        assert!(result.is_ok());
        let updated = result.unwrap();
        assert_eq!(updated.context, "Updated context");

        // Verify the change persisted
        let retrieved = ts.store.get(&task.task_id).await.unwrap().unwrap();
        assert_eq!(retrieved.context, "Updated context");
    }

    #[tokio::test]
    async fn test_update_nonexistent_fails() {
        let ts = create_store();
        let task = create_test_task("vacation in SF");

        // Try to update without creating first
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
        let ts = create_store();
        let task = create_test_task("vacation in SF");

        ts.store.create(task.clone()).await.unwrap();
        let task_dir = ts.store.task_dir(&task.task_id);
        assert!(task_dir.exists());

        // Delete the task
        let result = ts.store.delete(&task.task_id).await;

        assert!(result.is_ok());

        // Verify it no longer exists
        let retrieved = ts.store.get(&task.task_id).await.unwrap();
        assert!(retrieved.is_none());

        // Verify directory was removed
        assert!(!task_dir.exists());
    }

    #[tokio::test]
    async fn test_delete_nonexistent_fails() {
        let ts = create_store();

        let result = ts.store.delete("nonexistent_id").await;

        assert!(result.is_err());
        match result {
            Err(TaskStoreError::NotFound(id)) => {
                assert_eq!(id, "nonexistent_id");
            }
            _ => panic!("Expected NotFound error"),
        }
    }

    #[tokio::test]
    async fn test_exists() {
        let ts = create_store();
        let task = create_test_task("vacation in SF");

        // Should not exist initially
        let exists = ts.store.exists(&task.task_id).await.unwrap();
        assert!(!exists);

        // Create the task
        ts.store.create(task.clone()).await.unwrap();

        // Should exist now
        let exists = ts.store.exists(&task.task_id).await.unwrap();
        assert!(exists);

        // Random ID should not exist
        let exists = ts.store.exists("random_id").await.unwrap();
        assert!(!exists);
    }

    #[tokio::test]
    async fn test_count() {
        let ts = create_store();

        // Empty store should have count 0
        let count = ts.store.count().await.unwrap();
        assert_eq!(count, 0);

        // Add 3 tasks
        let task1 = create_test_task("First");
        let task2 = create_test_task("Second");
        let task3 = create_test_task("Third");

        ts.store.create(task1.clone()).await.unwrap();
        ts.store.create(task2.clone()).await.unwrap();
        ts.store.create(task3.clone()).await.unwrap();

        // Should have count 3
        let count = ts.store.count().await.unwrap();
        assert_eq!(count, 3);

        // Delete one task
        ts.store.delete(&task1.task_id).await.unwrap();

        // Should have count 2
        let count = ts.store.count().await.unwrap();
        assert_eq!(count, 2);
    }
}
