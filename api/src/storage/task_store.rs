//! Task storage abstraction layer
//!
//! This module provides trait-based abstraction for task persistence,
//! allowing multiple implementations (in-memory, database, etc.) without
//! changing business logic.

use async_trait::async_trait;
use thiserror::Error;

use crate::models::Task;

// ============================================================================
// Error Types
// ============================================================================

/// Errors that can occur during task storage operations.
#[derive(Debug, Error)]
pub enum TaskStoreError {
    /// The requested task was not found.
    #[error("Task not found: {0}")]
    NotFound(String),

    /// A task with the same ID already exists.
    #[error("Task already exists: {0}")]
    AlreadyExists(String),

    /// A generic storage error occurred.
    #[error("Storage error: {0}")]
    #[allow(dead_code)]
    StorageError(String),
}

/// Result type alias for task storage operations.
pub type TaskStoreResult<T> = Result<T, TaskStoreError>;

// ============================================================================
// TaskStore Trait
// ============================================================================

/// Trait defining the interface for task persistence.
///
/// This trait provides an abstract interface for storing and retrieving tasks.
/// Implementations can use different backends (in-memory, database, etc.).
///
/// ## Thread Safety
///
/// All implementations must be `Send + Sync` to support concurrent access
/// from multiple Tokio tasks without data races.
///
/// ## Example Implementation
///
/// ```ignore
/// use async_trait::async_trait;
/// use std::sync::Arc;
/// use dashmap::DashMap;
///
/// pub struct InMemoryTaskStore {
///     tasks: Arc<DashMap<String, Task>>,
/// }
///
/// #[async_trait]
/// impl TaskStore for InMemoryTaskStore {
///     async fn create(&self, task: Task) -> TaskStoreResult<Task> {
///         // Implementation here
///     }
///     // ... other methods
/// }
/// ```
#[async_trait]
pub trait TaskStore: Send + Sync {
    /// Creates a new task in the store.
    ///
    /// # Arguments
    /// * `task` - The task to create
    ///
    /// # Returns
    /// * `Ok(Task)` - The created task (same as input)
    /// * `Err(AlreadyExists)` - If a task with the same ID already exists
    /// * `Err(StorageError)` - If a storage-level error occurs
    ///
    /// # Example
    /// ```ignore
    /// let task = Task::new("vacation in SF".to_string());
    /// let created = store.create(task).await?;
    /// ```
    async fn create(&self, task: Task) -> TaskStoreResult<Task>;

    /// Retrieves a task by its ID.
    ///
    /// # Arguments
    /// * `task_id` - The unique identifier of the task
    ///
    /// # Returns
    /// * `Ok(Some(Task))` - The task if found
    /// * `Ok(None)` - If no task with the given ID exists
    /// * `Err(StorageError)` - If a storage-level error occurs
    ///
    /// # Example
    /// ```ignore
    /// match store.get("task_123").await? {
    ///     Some(task) => println!("Found: {:?}", task),
    ///     None => println!("Task not found"),
    /// }
    /// ```
    async fn get(&self, task_id: &str) -> TaskStoreResult<Option<Task>>;

    /// Lists all tasks in the store.
    ///
    /// # Returns
    /// * `Ok(Vec<Task>)` - Vector of all tasks (may be empty)
    /// * `Err(StorageError)` - If a storage-level error occurs
    ///
    /// # Note
    /// The order of tasks in the returned vector is implementation-defined.
    /// For consistent ordering, implementations should sort by `created_at`.
    ///
    /// # Example
    /// ```ignore
    /// let all_tasks = store.list().await?;
    /// println!("Total tasks: {}", all_tasks.len());
    /// ```
    async fn list(&self) -> TaskStoreResult<Vec<Task>>;

    /// Updates an existing task.
    ///
    /// # Arguments
    /// * `task` - The task with updated fields (task_id must match existing task)
    ///
    /// # Returns
    /// * `Ok(Task)` - The updated task
    /// * `Err(NotFound)` - If no task with the given ID exists
    /// * `Err(StorageError)` - If a storage-level error occurs
    ///
    /// # Note
    /// This performs a full replacement of the task. Only the `context` field
    /// is typically updated in practice, but the entire task is stored.
    ///
    /// # Example
    /// ```ignore
    /// let mut task = store.get("task_123").await?.unwrap();
    /// task.context = "Updated context".to_string();
    /// let updated = store.update(task).await?;
    /// ```
    async fn update(&self, task: Task) -> TaskStoreResult<Task>;

    /// Deletes a task from the store.
    ///
    /// # Arguments
    /// * `task_id` - The unique identifier of the task to delete
    ///
    /// # Returns
    /// * `Ok(())` - Task successfully deleted
    /// * `Err(NotFound)` - If no task with the given ID exists
    /// * `Err(StorageError)` - If a storage-level error occurs
    ///
    /// # Example
    /// ```ignore
    /// store.delete("task_123").await?;
    /// ```
    async fn delete(&self, task_id: &str) -> TaskStoreResult<()>;

    /// Checks if a task exists.
    ///
    /// # Arguments
    /// * `task_id` - The unique identifier to check
    ///
    /// # Returns
    /// * `Ok(true)` - Task exists
    /// * `Ok(false)` - Task does not exist
    /// * `Err(StorageError)` - If a storage-level error occurs
    ///
    /// # Note
    /// This is more efficient than calling `get()` when you only need to
    /// check existence without retrieving the task data.
    ///
    /// # Example
    /// ```ignore
    /// if store.exists("task_123").await? {
    ///     println!("Task exists");
    /// }
    /// ```
    #[allow(dead_code)]
    async fn exists(&self, task_id: &str) -> TaskStoreResult<bool>;

    /// Returns the total number of tasks in the store.
    ///
    /// # Returns
    /// * `Ok(usize)` - The count of tasks
    /// * `Err(StorageError)` - If a storage-level error occurs
    ///
    /// # Example
    /// ```ignore
    /// let count = store.count().await?;
    /// println!("Total tasks: {}", count);
    /// ```
    #[allow(dead_code)]
    async fn count(&self) -> TaskStoreResult<usize>;
}
